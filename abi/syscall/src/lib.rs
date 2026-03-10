// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

//! Syscall ABI definitions — single source of truth.
//!
//! This crate defines the binary interface between userspace and the kernel.
//! See `kernel/docs/syscalls.md` for the full specification.
//!
//! # Register conventions
//!
//! **x86-64** (`SYSCALL`/`SYSRET`):
//! - `rax` — syscall number (in); return value (out; negative = error)
//! - `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9` — arguments 0–5
//! - `rcx` clobbered by `SYSCALL` (holds return address); not an arg register
//! - `r11` clobbered by `SYSCALL` (holds saved rflags); not an arg register
//! - `rdx` — secondary return value (e.g. word count from `ipc_recv`)
//!
//! **RISC-V** (`ECALL`):
//! - `a7` — syscall number
//! - `a0`–`a5` — arguments 0–5
//! - `a0` — primary return value (negative = error)
//! - `a1` — secondary return value
//!
//! # Rules
//! - No std; builds in `no_std`.
//! - No inline assembly.
//! - All cross-boundary types are `#[repr(C)]` with stable layout.
//! - No dependencies outside `core`.

#![no_std]

// ── Syscall numbers ───────────────────────────────────────────────────────────

/// IPC: synchronous call (send + block waiting for reply).
pub const SYS_IPC_CALL: u64 = 0;
/// IPC: reply to a pending call.
pub const SYS_IPC_REPLY: u64 = 1;
/// IPC: receive a call on an endpoint.
pub const SYS_IPC_RECV: u64 = 2;
/// Signal: send (OR bits into signal object).
pub const SYS_SIGNAL_SEND: u64 = 3;
/// Signal: wait (read-and-clear; blocks if zero).
pub const SYS_SIGNAL_WAIT: u64 = 4;
/// EventQueue: post an entry.
pub const SYS_EVENT_POST: u64 = 5;
/// EventQueue: receive an entry.
pub const SYS_EVENT_RECV: u64 = 6;
/// Capability: create an Endpoint object.
pub const SYS_CAP_CREATE_ENDPOINT: u64 = 7;
/// Capability: create a Signal object.
pub const SYS_CAP_CREATE_SIGNAL: u64 = 8;
/// Capability: create an EventQueue object.
pub const SYS_CAP_CREATE_EVENT_Q: u64 = 9;
/// Capability: create a Thread object.
pub const SYS_CAP_CREATE_THREAD: u64 = 10;
/// Capability: create an AddressSpace object.
pub const SYS_CAP_CREATE_ASPACE: u64 = 11;
/// Capability: create a CSpace object.
pub const SYS_CAP_CREATE_CSPACE: u64 = 12;
/// Capability: create a WaitSet object.
pub const SYS_CAP_CREATE_WAIT_SET: u64 = 13;
/// Capability: derive (attenuate rights).
pub const SYS_CAP_DERIVE: u64 = 14;
/// Capability: revoke a capability and all descendants.
pub const SYS_CAP_REVOKE: u64 = 15;
/// Memory: map a Frame into an address space.
pub const SYS_MEM_MAP: u64 = 16;
/// Memory: unmap a region from an address space.
pub const SYS_MEM_UNMAP: u64 = 17;
/// Memory: change protections on a mapped region.
pub const SYS_MEM_PROTECT: u64 = 18;
/// Thread: start execution.
pub const SYS_THREAD_START: u64 = 19;
/// Thread: stop execution.
pub const SYS_THREAD_STOP: u64 = 20;
/// Thread: yield the CPU.
pub const SYS_THREAD_YIELD: u64 = 21;
/// Thread: exit and free TCB.
pub const SYS_THREAD_EXIT: u64 = 22;
/// Thread: configure (set entry, stack, arg).
pub const SYS_THREAD_CONFIGURE: u64 = 23;
/// Capability: copy a slot.
pub const SYS_CAP_COPY: u64 = 24;
/// Capability: move a slot (destroying the source).
pub const SYS_CAP_MOVE: u64 = 25;
/// WaitSet: add a member.
pub const SYS_WAIT_SET_ADD: u64 = 26;
/// WaitSet: remove a member.
pub const SYS_WAIT_SET_REMOVE: u64 = 27;
/// WaitSet: wait for any member to become ready.
pub const SYS_WAIT_SET_WAIT: u64 = 28;
/// IRQ: acknowledge a delivered interrupt.
pub const SYS_IRQ_ACK: u64 = 29;
/// IRQ: register a signal to receive interrupt notifications.
pub const SYS_IRQ_REGISTER: u64 = 30;
/// Capability: delete a slot.
pub const SYS_CAP_DELETE: u64 = 31;
/// Capability: insert an object into a specific slot.
pub const SYS_CAP_INSERT: u64 = 32;
/// Frame: split a large frame into smaller ones.
pub const SYS_FRAME_SPLIT: u64 = 33;
/// Memory: map an MMIO region.
pub const SYS_MMIO_MAP: u64 = 34;
/// I/O: bind an IoPortRange to the calling thread.
pub const SYS_IOPORT_BIND: u64 = 35;
/// DMA: grant a frame for DMA use.
pub const SYS_DMA_GRANT: u64 = 36;
/// Thread: set scheduling priority.
pub const SYS_THREAD_SET_PRIORITY: u64 = 37;
/// Thread: set CPU affinity.
pub const SYS_THREAD_SET_AFFINITY: u64 = 38;
/// Thread: read register state (debug / ptrace).
pub const SYS_THREAD_READ_REGS: u64 = 39;
/// Thread: write register state (debug / ptrace).
pub const SYS_THREAD_WRITE_REGS: u64 = 40;
/// AddressSpace: query mapping information.
pub const SYS_ASPACE_QUERY: u64 = 41;
/// IPC: set the IPC buffer address for the calling thread.
pub const SYS_IPC_BUFFER_SET: u64 = 42;
/// System: query kernel capabilities / version.
pub const SYS_SYSTEM_INFO: u64 = 43;
/// Debug: write a UTF-8 string to the kernel console (temporary).
pub const SYS_DEBUG_LOG: u64 = 44;

// ── Error codes ───────────────────────────────────────────────────────────────

/// Syscall error codes returned in `rax` / `a0` as negative `i64` values.
///
/// On success the return value is `>= 0`. On error it is one of these
/// negative values. Userspace wrappers check `rax < 0` to detect errors.
#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyscallError
{
    /// Syscall number is not valid.
    UnknownSyscall = -1,
    /// Capability slot index is out of range or slot is null.
    InvalidCapability = -2,
    /// Caller does not hold sufficient rights for this operation.
    InsufficientRights = -3,
    /// A pointer argument does not satisfy alignment or range requirements.
    InvalidAddress = -4,
    /// An integer argument is out of the valid range for this call.
    InvalidArgument = -5,
    /// The operation would block but the caller requested non-blocking mode.
    WouldBlock = -6,
    /// The target thread or object has already exited / been destroyed.
    ObjectGone = -7,
    /// No memory available to satisfy the request.
    OutOfMemory = -8,
    /// The operation is not supported on this object type.
    NotSupported = -9,
    /// The capability rights bitmask violates the W^X constraint.
    WxViolation = -10,
    /// The message is too large for the destination.
    MsgTooLarge = -11,
    /// Deadlock would occur (IPC cycle detected).
    Deadlock = -12,
}

// ── Message constants ─────────────────────────────────────────────────────────

/// Maximum number of data words in an IPC message.
pub const MSG_DATA_WORDS_MAX: usize = 6;

/// Maximum number of capability slots transferable in a single IPC message.
pub const MSG_CAP_SLOTS_MAX: usize = 4;

/// Maximum number of registers used for inline message data (x86-64: rdi–r9).
/// Words beyond this limit require an IPC buffer in shared memory.
pub const MSG_REGS_DATA_MAX: usize = 6;

// ── System info ───────────────────────────────────────────────────────────────

/// Discriminant for `SYS_SYSTEM_INFO` queries.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemInfoKind
{
    /// Return the kernel protocol version number.
    KernelVersion = 0,
    /// Return the number of CPUs detected.
    CpuCount = 1,
    /// Return the number of free physical frames.
    FreeFrames = 2,
}
