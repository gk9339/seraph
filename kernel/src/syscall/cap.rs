// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/cap.rs

//! Capability creation syscall handlers — Phase 10.
//!
//! Allocates kernel objects and inserts them into the current thread's CSpace.
//! Returns the slot index on success.

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use alloc::boxed::Box;

use crate::arch::current::trap_frame::TrapFrame;
use syscall::SyscallError;

#[cfg(not(test))]
use super::current_tcb;

/// SYS_CAP_CREATE_ENDPOINT (7): create a new Endpoint object.
///
/// Allocates `EndpointState` and `EndpointObject`, inserts a cap with
/// `SEND | RECEIVE | GRANT` rights into the current thread's CSpace.
/// Returns the slot index in rax/a0.
#[cfg(not(test))]
pub fn sys_cap_create_endpoint(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::{EndpointObject, KernelObjectHeader, ObjectType};
    use crate::cap::slot::{CapTag, Rights};
    use crate::ipc::endpoint::EndpointState;
    use core::ptr::NonNull;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: tcb is valid from syscall context.
    let cspace = unsafe { (*tcb).cspace };
    if cspace.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }

    // Allocate EndpointState.
    let ep_state_ptr = Box::into_raw(Box::new(EndpointState::new()));

    // Allocate EndpointObject (header at offset 0 for safe *-to-header cast).
    let ep_obj_ptr = Box::into_raw(Box::new(EndpointObject {
        header: KernelObjectHeader::new(ObjectType::Endpoint),
        state:  ep_state_ptr,
    }));

    // Build NonNull<KernelObjectHeader> by casting (header is at offset 0).
    let nonnull = unsafe { NonNull::new_unchecked(ep_obj_ptr as *mut KernelObjectHeader) };

    // Insert into CSpace.
    let idx = unsafe { (*cspace).insert_cap(CapTag::Endpoint, Rights::SEND | Rights::RECEIVE | Rights::GRANT, nonnull) }
        .map_err(|_| SyscallError::OutOfMemory)?;

    Ok(idx as u64)
}

/// SYS_CAP_CREATE_SIGNAL (8): create a new Signal object.
///
/// Allocates `SignalState` and `SignalObject`, inserts a cap with
/// `SIGNAL | WAIT` rights into the current thread's CSpace.
/// Returns the slot index in rax/a0.
#[cfg(not(test))]
pub fn sys_cap_create_signal(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::{KernelObjectHeader, ObjectType, SignalObject};
    use crate::cap::slot::{CapTag, Rights};
    use crate::ipc::signal::SignalState;
    use core::ptr::NonNull;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace = unsafe { (*tcb).cspace };
    if cspace.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }

    // Allocate SignalState.
    let sig_state_ptr = Box::into_raw(Box::new(SignalState::new()));

    // Allocate SignalObject.
    let sig_obj_ptr = Box::into_raw(Box::new(SignalObject {
        header: KernelObjectHeader::new(ObjectType::Signal),
        state:  sig_state_ptr,
    }));

    let nonnull = unsafe { NonNull::new_unchecked(sig_obj_ptr as *mut KernelObjectHeader) };

    let idx = unsafe { (*cspace).insert_cap(CapTag::Signal, Rights::SIGNAL | Rights::WAIT, nonnull) }
        .map_err(|_| SyscallError::OutOfMemory)?;

    Ok(idx as u64)
}

// Stubs for test builds (these handlers are not called in host tests).
#[cfg(test)]
pub fn sys_cap_create_endpoint(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

#[cfg(test)]
pub fn sys_cap_create_signal(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}
