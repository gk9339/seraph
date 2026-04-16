// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/ipc/src/lib.rs

//! Shared IPC helpers for Seraph userspace services.
//!
//! Provides common patterns used across multiple services: sentinel constants
//! for capability identification, IPC buffer path packing/unpacking, capability
//! injection into child `CSpaces`, and `ProcessInfo` descriptor writing.

#![no_std]
// cast_possible_truncation: userspace targets 64-bit only; u64/usize conversions
// are lossless. u32 casts on capability slot indices are bounded by CSpace capacity.
#![allow(clippy::cast_possible_truncation)]

use process_abi::{CapDescriptor, CapType, ProcessInfo};

// ── Sentinel constants ──────────────────────────────────────────────────────
//
// These values are placed in `CapDescriptor.aux0` to identify well-known
// endpoint capabilities during startup cap classification. Each service
// iterates its initial caps and matches `aux0` against these sentinels.

/// Log endpoint (IPC logging sink).
pub const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// Service protocol endpoint (the service's own receive endpoint).
pub const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;

/// Device registry endpoint (devmgr query interface).
pub const REGISTRY_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFD;

/// Block device endpoint (virtio-blk service interface).
pub const BLOCK_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFC;

/// Process manager endpoint (procmgr create/start interface).
pub const PROCMGR_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFB;

// ── IPC label constants ─────────────────────────────────────────────────────
//
// Per-service IPC operation labels. Namespaced by service because label
// numbers are only meaningful relative to a specific endpoint.

/// IPC labels for the process manager (`procmgr`).
pub mod procmgr_labels
{
    /// Create a new process from a boot module frame.
    pub const CREATE_PROCESS: u64 = 1;
    /// Start a previously created (suspended) process.
    pub const START_PROCESS: u64 = 2;
    /// Request physical memory frames from procmgr's pool.
    pub const REQUEST_FRAMES: u64 = 5;
    /// Create a new process from a VFS path (ELF binary).
    pub const CREATE_FROM_VFS: u64 = 6;
    /// Provide procmgr with the vfsd endpoint for VFS-based loading.
    pub const SET_VFSD_EP: u64 = 7;
}

/// IPC labels for the service manager (`svcmgr`).
pub mod svcmgr_labels
{
    /// Register a service for health monitoring.
    pub const REGISTER_SERVICE: u64 = 1;
    /// Signal that init handover is complete.
    pub const HANDOVER_COMPLETE: u64 = 2;
}

/// IPC labels for the VFS daemon (`vfsd`).
pub mod vfsd_labels
{
    /// Open a file by path.
    pub const OPEN: u64 = 1;
    /// Read from an open file.
    pub const READ: u64 = 2;
    /// Close an open file.
    pub const CLOSE: u64 = 3;
    /// Stat an open file (get size/attributes).
    pub const STAT: u64 = 4;
    /// Read a directory entry.
    pub const READDIR: u64 = 5;
    /// Mount a filesystem at a path.
    pub const MOUNT: u64 = 10;
}

/// IPC labels for filesystem drivers (FAT, ext4, etc.).
pub mod fs_labels
{
    /// Open a file by path (driver-side).
    pub const FS_OPEN: u64 = 1;
    /// Read from an open file (driver-side).
    pub const FS_READ: u64 = 2;
    /// Close an open file (driver-side).
    pub const FS_CLOSE: u64 = 3;
    /// Stat an open file (driver-side).
    pub const FS_STAT: u64 = 4;
    /// Read a directory entry (driver-side).
    pub const FS_READDIR: u64 = 5;
    /// End-of-directory marker in readdir replies.
    pub const END_OF_DIR: u64 = 6;
    /// Mount notification from vfsd.
    pub const FS_MOUNT: u64 = 10;
}

/// IPC labels for the device manager (`devmgr`).
pub mod devmgr_labels
{
    /// Query for a block device endpoint.
    pub const QUERY_BLOCK_DEVICE: u64 = 1;
}

/// IPC labels for block device drivers.
pub mod blk_labels
{
    /// Read a single sector (512 bytes).
    pub const READ_BLOCK: u64 = 1;
}

// ── Path codec ──────────────────────────────────────────────────────────────

/// Maximum path length in bytes (6 IPC data words = 48 bytes).
pub const MAX_PATH_LEN: usize = 48;

/// Maximum data words used for a path (6 words of 8 bytes each).
const MAX_PATH_WORDS: usize = 6;

/// Read path bytes from IPC buffer data words.
///
/// Unpacks `path_len` bytes from little-endian u64 words starting at `ipc_buf`.
/// Writes into `buf`; returns the number of bytes written (capped at `buf.len()`
/// and `path_len`).
///
/// # Safety
///
/// `ipc_buf` must point to a valid IPC buffer region with at least
/// `path_len.div_ceil(8)` readable words.
pub unsafe fn read_path_from_ipc(ipc_buf: *const u64, path_len: usize, buf: &mut [u8]) -> usize
{
    let effective_len = path_len.min(buf.len()).min(MAX_PATH_LEN);
    let word_count = effective_len.div_ceil(8).min(MAX_PATH_WORDS);
    for i in 0..word_count
    {
        // SAFETY: caller guarantees ipc_buf has enough readable words.
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(i)) };
        let base = i * 8;
        for j in 0..8
        {
            if base + j < effective_len
            {
                buf[base + j] = (word >> (j * 8)) as u8;
            }
        }
    }
    effective_len
}

/// Write path bytes into IPC buffer data words.
///
/// Packs `path` bytes into little-endian u64 words starting at `ipc_buf`.
/// Returns the number of words written.
///
/// # Safety
///
/// `ipc_buf` must point to a valid IPC buffer region with at least
/// `path.len().div_ceil(8)` writable words.
pub unsafe fn write_path_to_ipc(ipc_buf: *mut u64, path: &[u8]) -> usize
{
    let effective_len = path.len().min(MAX_PATH_LEN);
    let word_count = effective_len.div_ceil(8).min(MAX_PATH_WORDS);
    for i in 0..word_count
    {
        let mut word: u64 = 0;
        let base = i * 8;
        for j in 0..8
        {
            if base + j < effective_len
            {
                word |= u64::from(path[base + j]) << (j * 8);
            }
        }
        // SAFETY: caller guarantees ipc_buf has enough writable words.
        unsafe { core::ptr::write_volatile(ipc_buf.add(i), word) };
    }
    word_count
}

// ── Cap injection ───────────────────────────────────────────────────────────

/// Tracks state while injecting capabilities into a child `CSpace`.
///
/// Groups the descriptor buffer, count, and first-slot sentinel that every cap
/// injection site needs. Pass to [`inject_cap`] instead of separate out-params.
pub struct CapInjector<'a>
{
    /// Descriptor buffer to append to.
    pub descs: &'a mut [CapDescriptor],
    /// Number of descriptors written so far.
    pub count: usize,
    /// Slot index of the first injected cap (`u32::MAX` = none yet).
    pub first_slot: u32,
}

impl<'a> CapInjector<'a>
{
    /// Create a new injector over the given descriptor buffer.
    pub fn new(descs: &'a mut [CapDescriptor]) -> Self
    {
        Self {
            descs,
            count: 0,
            first_slot: u32::MAX,
        }
    }
}

/// Derive and copy a capability into a child `CSpace`, recording a descriptor.
///
/// Derives an intermediary from `src_slot` with the given `rights`, copies it
/// into `child_cspace`, and appends a [`CapDescriptor`] to the injector. Updates
/// the injector's count and first-slot on the first successful injection.
///
/// Returns the child slot index on success, or `None` if derivation or copy
/// fails.
pub fn inject_cap(
    src_slot: u32,
    rights: u64,
    cap_type: CapType,
    aux0: u64,
    aux1: u64,
    child_cspace: u32,
    inj: &mut CapInjector<'_>,
) -> Option<u32>
{
    let intermediary = syscall::cap_derive(src_slot, rights).ok()?;
    let child_slot = syscall::cap_copy(intermediary, child_cspace, rights).ok()?;

    if inj.first_slot == u32::MAX
    {
        inj.first_slot = child_slot;
    }
    if inj.count < inj.descs.len()
    {
        inj.descs[inj.count] = CapDescriptor {
            slot: child_slot,
            cap_type,
            pad: [0; 3],
            aux0,
            aux1,
        };
        inj.count += 1;
    }
    Some(child_slot)
}

// ── ProcessInfo descriptor writer ───────────────────────────────────────────

/// Page size constant (4 KiB).
const PAGE_SIZE: usize = 0x1000;

/// Write capability descriptors into a mapped `ProcessInfo` page.
///
/// Sets `initial_caps_base`, `initial_caps_count`, `cap_descriptor_count`,
/// and `cap_descriptors_offset` on the `ProcessInfo` header, then writes the
/// descriptor entries after the header (8-byte aligned).
///
/// `pi_va` must be a virtual address where the `ProcessInfo` page is mapped
/// writable. Only writes as many descriptors as fit within the page.
///
/// # Safety
///
/// `pi_va` must point to a writable, page-aligned mapping of a `ProcessInfo`
/// page. The page must remain mapped for the duration of this call.
pub unsafe fn write_cap_descriptors(
    pi_va: u64,
    desc_buf: &[CapDescriptor],
    desc_count: usize,
    first_slot: u32,
)
{
    // SAFETY: pi_va is mapped writable by the caller.
    // cast_ptr_alignment: pi_va is page-aligned (4096-byte).
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(pi_va as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;

    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    // Align to 8 bytes (CapDescriptor contains u64 fields).
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    let desc_size = core::mem::size_of::<CapDescriptor>();
    let mut written: usize = 0;
    for (i, desc) in desc_buf.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE
        {
            break;
        }
        // SAFETY: byte_offset is within the mapped page; descs_offset_aligned is
        // 8-byte aligned and CapDescriptor is 24 bytes (multiple of 8).
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (pi_va as *mut u8).add(byte_offset).cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
        written += 1;
    }
    pi.initial_caps_count = written as u32;
    pi.cap_descriptor_count = written as u32;
}
