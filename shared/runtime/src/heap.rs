// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/runtime/src/heap.rs

//! Per-process heap allocator for userspace.
//!
//! Linked-list free-list allocator with first-fit allocation and
//! coalesce-on-free. The heap occupies a fixed VA range from
//! [`va_layout::HEAP_BASE`] to [`va_layout::HEAP_MAX`]. Initial frames are
//! requested from procmgr at [`bootstrap_from_procmgr`] time.
//!
//! The allocator serves as `#[global_allocator]`: `Box`, `Vec`, `String`,
//! `BTreeMap`, and every other `alloc` crate type goes through here.
//!
//! # Concurrency
//!
//! The allocator is guarded by a spinlock. Multi-threaded services (init
//! main + log thread; vfsd main + worker) share a single allocator; the
//! spinlock serialises access.

// cast_possible_truncation: userspace targets are 64-bit; usize == u64.
// used_underscore_items: `runtime::log!` expands to `_log_fmt`, which is
// module-internal API and deliberately underscore-prefixed.
#![allow(clippy::cast_possible_truncation, clippy::used_underscore_items)]

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::mem::{align_of, size_of};
use core::ptr::{null_mut, NonNull};
use core::sync::atomic::{AtomicBool, Ordering};

use ipc::{procmgr_labels, IpcBuf};

// ── Spinlock ────────────────────────────────────────────────────────────────

struct SpinLock
{
    locked: AtomicBool,
}

impl SpinLock
{
    const fn new() -> Self
    {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    fn lock(&self)
    {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn unlock(&self)
    {
        self.locked.store(false, Ordering::Release);
    }
}

// ── Free-list node ──────────────────────────────────────────────────────────

#[repr(C)]
struct FreeNode
{
    size: usize,
    next: Option<NonNull<FreeNode>>,
}

const NODE_SIZE: usize = size_of::<FreeNode>();
const NODE_ALIGN: usize = align_of::<FreeNode>();
const MIN_BLOCK: usize = NODE_SIZE;

fn align_up(addr: usize, align: usize) -> usize
{
    (addr + align - 1) & !(align - 1)
}

// ── Allocator state ─────────────────────────────────────────────────────────

struct Heap
{
    head: Option<NonNull<FreeNode>>,
}

impl Heap
{
    const fn new() -> Self
    {
        Self { head: None }
    }

    /// # Safety
    ///
    /// `base..base+size` must be a writable, exclusively-owned region that
    /// stays mapped for the process's lifetime.
    unsafe fn init(&mut self, base: usize, size: usize)
    {
        if size < NODE_SIZE
        {
            return;
        }
        let node = base as *mut FreeNode;
        // SAFETY: caller guarantees the region is writable.
        unsafe {
            (*node).size = size;
            (*node).next = None;
        }
        self.head = NonNull::new(node);
    }

    /// # Safety
    ///
    /// `ptr..ptr+size` must be a freed allocation previously returned by
    /// [`Self::alloc`] from this heap.
    unsafe fn insert(&mut self, ptr: usize, size: usize)
    {
        let mut cur = self.head;
        let mut prev: Option<NonNull<FreeNode>> = None;
        while let Some(c) = cur
        {
            if (c.as_ptr() as usize) > ptr
            {
                break;
            }
            prev = Some(c);
            // SAFETY: c is a valid free node in our list.
            cur = unsafe { (*c.as_ptr()).next };
        }

        let new_node = ptr as *mut FreeNode;
        // SAFETY: caller guarantees the region is writable.
        unsafe {
            (*new_node).size = size;
            (*new_node).next = cur;
        }

        match prev
        {
            Some(p) =>
            {
                // SAFETY: p is a valid node we just walked past.
                unsafe { (*p.as_ptr()).next = NonNull::new(new_node) };
            }
            None => self.head = NonNull::new(new_node),
        }

        let inserted = NonNull::new(new_node).unwrap();
        // SAFETY: inserted is freshly placed in the list.
        unsafe { Self::try_coalesce(inserted) };
        if let Some(p) = prev
        {
            // SAFETY: p is a valid node in our list.
            unsafe { Self::try_coalesce(p) };
        }
    }

    /// # Safety
    ///
    /// `node` must point to a valid free-list node.
    unsafe fn try_coalesce(node: NonNull<FreeNode>)
    {
        // SAFETY: caller guarantees `node` is valid.
        unsafe {
            let n = node.as_ptr();
            let Some(next) = (*n).next
            else
            {
                return;
            };
            let next_ptr = next.as_ptr() as usize;
            let n_end = node.as_ptr() as usize + (*n).size;
            if n_end == next_ptr
            {
                (*n).size += (*next.as_ptr()).size;
                (*n).next = (*next.as_ptr()).next;
            }
        }
    }

    /// First-fit allocation. Returns null if no block satisfies `layout`.
    fn alloc(&mut self, layout: Layout) -> *mut u8
    {
        let want = layout.size().max(MIN_BLOCK);
        let align = layout.align().max(NODE_ALIGN);
        let mut prev: Option<NonNull<FreeNode>> = None;
        let mut cur = self.head;
        while let Some(c) = cur
        {
            // SAFETY: c is a valid free node.
            let (node_size, node_next) = unsafe { ((*c.as_ptr()).size, (*c.as_ptr()).next) };
            let start = c.as_ptr() as usize;
            let payload = align_up(start, align);
            let padding = payload - start;
            if node_size >= padding + want
            {
                let total_used = padding + want;
                let remaining = node_size - total_used;
                if remaining >= MIN_BLOCK
                {
                    let new_node_addr = start + total_used;
                    let new_node = new_node_addr as *mut FreeNode;
                    // SAFETY: address is inside the original free block.
                    unsafe {
                        (*new_node).size = remaining;
                        (*new_node).next = node_next;
                    }
                    let replacement = NonNull::new(new_node);
                    match prev
                    {
                        Some(p) =>
                        // SAFETY: p is a valid node we walked past.
                        unsafe {
                            (*p.as_ptr()).next = replacement;
                        },
                        None => self.head = replacement,
                    }
                }
                else
                {
                    match prev
                    {
                        Some(p) =>
                        // SAFETY: p is a valid node we walked past.
                        unsafe {
                            (*p.as_ptr()).next = node_next;
                        },
                        None => self.head = node_next,
                    }
                }
                return payload as *mut u8;
            }
            prev = Some(c);
            cur = node_next;
        }
        null_mut()
    }
}

// ── Global heap ─────────────────────────────────────────────────────────────

struct GlobalHeap
{
    inner: UnsafeCell<Heap>,
    lock: SpinLock,
}

// SAFETY: all access to `inner` goes through `lock`.
unsafe impl Sync for GlobalHeap {}

impl GlobalHeap
{
    const fn new() -> Self
    {
        Self {
            inner: UnsafeCell::new(Heap::new()),
            lock: SpinLock::new(),
        }
    }
}

// SAFETY: correctness follows from the free-list invariants upheld by
// `Heap::alloc` and `Heap::insert` under `lock`.
unsafe impl GlobalAlloc for GlobalHeap
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8
    {
        self.lock.lock();
        // SAFETY: lock held; single mutator.
        let ptr = unsafe { (*self.inner.get()).alloc(layout) };
        self.lock.unlock();
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
    {
        if ptr.is_null()
        {
            return;
        }
        let size = layout.size().max(MIN_BLOCK);
        self.lock.lock();
        // SAFETY: lock held; caller guarantees ptr/layout match a prior alloc.
        unsafe { (*self.inner.get()).insert(ptr as usize, size) };
        self.lock.unlock();
    }
}

#[global_allocator]
static HEAP: GlobalHeap = GlobalHeap::new();

// ── Init from procmgr ───────────────────────────────────────────────────────

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialise the heap by requesting frames from procmgr and mapping them
/// at [`va_layout::HEAP_BASE`]. Safe to call multiple times; only the
/// first succeeds. Returns `true` if the heap is now usable.
pub fn bootstrap_from_procmgr(procmgr_ep: u32, self_aspace: u32, ipc: IpcBuf) -> bool
{
    if INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return true;
    }

    if procmgr_ep == 0
    {
        crate::log!("heap: bootstrap failed: procmgr_ep=0");
        return false;
    }

    // procmgr's REQUEST_FRAMES caps out at FRAMES_PER_REQUEST per call.
    // Loop to accumulate HEAP_INITIAL_PAGES, mapping each page as it arrives.
    let mut mapped: u64 = 0;
    while mapped < va_layout::HEAP_INITIAL_PAGES
    {
        let want = (va_layout::HEAP_INITIAL_PAGES - mapped).min(va_layout::FRAMES_PER_REQUEST);
        ipc.write_word(0, want);
        let Ok((label, _)) = syscall::ipc_call(procmgr_ep, procmgr_labels::REQUEST_FRAMES, 1, &[])
        else
        {
            crate::log!("heap: bootstrap failed: REQUEST_FRAMES ipc_call err");
            return false;
        };
        if label != 0
        {
            crate::log!("heap: bootstrap failed: REQUEST_FRAMES label={}", label);
            return false;
        }

        // SAFETY: ipc wraps the registered IPC buffer; kernel wrote cap metadata.
        let (cap_count, cap_slots) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
        if (cap_count as u64) < want
        {
            crate::log!(
                "heap: bootstrap failed: got {} caps, wanted {}",
                cap_count as u64,
                want
            );
            return false;
        }

        for i in 0..want
        {
            let va = va_layout::HEAP_BASE + (mapped + i) * 0x1000;
            if syscall::mem_map(
                cap_slots[i as usize],
                self_aspace,
                va,
                0,
                1,
                syscall::MAP_WRITABLE,
            )
            .is_err()
            {
                crate::log!("heap: bootstrap failed: mem_map err at page {}", mapped + i);
                return false;
            }
        }
        mapped += want;
    }

    let base = va_layout::HEAP_BASE as usize;
    let size = (va_layout::HEAP_INITIAL_PAGES as usize) * 0x1000;
    HEAP.lock.lock();
    // SAFETY: the region is freshly mapped and exclusively owned; we hold
    // the heap lock and have gated entry on INITIALIZED.
    unsafe { (*HEAP.inner.get()).init(base, size) };
    HEAP.lock.unlock();
    true
}

/// Query whether the heap has been initialised.
#[must_use]
pub fn is_initialized() -> bool
{
    INITIALIZED.load(Ordering::Acquire)
}

// ── OOM path ────────────────────────────────────────────────────────────────
//
// On allocation failure, `GlobalAlloc::alloc` returns null. The `alloc`
// crate's infrastructure then calls `handle_alloc_error`, which panics.
// Our `panic_handler` (in `lib.rs`) exits the thread; svcmgr observes the
// death via its event queue and applies the configured restart policy.
//
// No `#[alloc_error_handler]` is installed: the default panic path is the
// desired behaviour (terminate on OOM, no recovery).
