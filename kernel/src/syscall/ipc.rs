// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/ipc.rs

//! IPC syscall handlers — Phase 9 stubs.
//!
//! Full IPC requires a running scheduler and capability lookup to find the
//! target Endpoint or Signal kernel object. These stubs return `NotSupported`
//! until capability lookups and the scheduler are wired together.
//!
//! # TODO Phase 9 (full wiring)
//! - Look up the cap slot from arg0 in the current thread's CSpace.
//! - Verify the cap tag (Endpoint / Signal) and required rights.
//! - Call the IPC kernel functions in `crate::ipc`.
//! - Enqueue/dequeue threads in the scheduler as appropriate.

use crate::arch::current::trap_frame::TrapFrame;
use syscall::SyscallError;

/// SYS_IPC_CALL (0): synchronous call on an endpoint.
pub fn sys_ipc_call(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // TODO Phase 9: cap lookup → endpoint_call → block caller on reply.
    Err(SyscallError::NotSupported)
}

/// SYS_IPC_REPLY (1): reply to a blocked caller.
pub fn sys_ipc_reply(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // TODO Phase 9: retrieve reply_tcb → endpoint_reply → wake caller.
    Err(SyscallError::NotSupported)
}

/// SYS_IPC_RECV (2): receive the next message on an endpoint.
pub fn sys_ipc_recv(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // TODO Phase 9: cap lookup → endpoint_recv → block server if empty.
    Err(SyscallError::NotSupported)
}

/// SYS_SIGNAL_SEND (3): OR bits into a signal object.
pub fn sys_signal_send(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // TODO Phase 9: cap lookup → signal_send → wake waiter if present.
    Err(SyscallError::NotSupported)
}

/// SYS_SIGNAL_WAIT (4): block until a signal bit is set.
pub fn sys_signal_wait(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // TODO Phase 9: cap lookup → signal_wait → block if bits = 0.
    Err(SyscallError::NotSupported)
}
