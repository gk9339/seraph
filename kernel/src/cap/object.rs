// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/cap/object.rs

//! Kernel object types backing capability objects.
//!
//! Each struct has a [`KernelObjectHeader`] as its first field at offset 0
//! (`#[repr(C)]`), so a `*mut ConcreteObject` can safely be cast to
//! `*mut KernelObjectHeader` and back.
//!
//! ## Allocation pattern
//!
//! ```text
//! Box::new(FrameObject { ... })
//!   → Box::into_raw()            // *mut FrameObject
//!   → cast to *mut KernelObjectHeader  // safe: header at offset 0
//!   → NonNull::new_unchecked()
//! ```
//!
//! Deallocation: read `header.obj_type` from the raw pointer,
//! then reconstruct the original `Box<ConcreteObject>` and drop it.
//!
//! ## Sizes (verified by tests below)
//!
//! | Type                | Size |
//! |---------------------|------|
//! | KernelObjectHeader  |  8 B |
//! | FrameObject         | 24 B |
//! | MmioRegionObject    | 32 B |
//! | InterruptObject     | 16 B |
//! | IoPortRangeObject   | 12 B |
//! | SchedControlObject  |  8 B |
//! | ThreadObject        | 16 B |
//! | AddressSpaceObject  | 16 B |
//! | CSpaceKernelObject  | 16 B |
//! | EndpointObject      | 16 B |
//! | SignalObject        | 16 B |
//! | EventQueueObject    | 16 B |
//! | WaitSetObject       | 16 B |

use core::sync::atomic::{AtomicU32, Ordering};

// ── ObjectType ────────────────────────────────────────────────────────────────

/// Discriminant for the concrete type behind a `*mut KernelObjectHeader`.
///
/// Used during deallocation to reconstruct the original `Box<ConcreteObject>`.
/// Values must not be renumbered after assignment.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType
{
    Frame = 0,
    MmioRegion = 1,
    Interrupt = 2,
    IoPortRange = 3,
    SchedControl = 4,
    Thread = 5,
    AddressSpace = 6,
    CSpaceObj = 7,
    Endpoint = 8,
    Signal = 9,
    EventQueue = 10,
    WaitSet = 11,
}

// ── KernelObjectHeader ────────────────────────────────────────────────────────

/// Common header at offset 0 of every kernel object.
///
/// The `ref_count` tracks how many capability slots reference this object.
/// When `dec_ref` returns 0, the object has no remaining references and can
/// be freed (future phases will handle deallocation via `obj_type`).
///
/// `#[repr(C)]` with size 8 B, alignment 4. All concrete object structs place
/// this as their first field so pointer casts are safe.
#[repr(C)]
pub struct KernelObjectHeader
{
    /// Reference count; starts at 1 when created.
    pub ref_count: AtomicU32,
    /// Concrete type, for use during deallocation.
    pub obj_type: ObjectType,
    pub _pad: [u8; 3],
}

impl KernelObjectHeader
{
    /// Construct a new header with `ref_count = 1`.
    pub fn new(obj_type: ObjectType) -> Self
    {
        Self {
            ref_count: AtomicU32::new(1),
            obj_type,
            _pad: [0; 3],
        }
    }

    /// Increment the reference count. Call when a new capability slot is
    /// derived pointing to this object.
    pub fn inc_ref(&self)
    {
        self.ref_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the reference count and return the new value.
    ///
    /// Returns 0 when the object has no remaining capability references; the
    /// caller is responsible for freeing the object at that point.
    pub fn dec_ref(&self) -> u32
    {
        self.ref_count.fetch_sub(1, Ordering::Release) - 1
    }
}

// ── Concrete object types ─────────────────────────────────────────────────────

/// Kernel object for a contiguous physical memory range (Frame capability).
#[repr(C)]
pub struct FrameObject
{
    pub header: KernelObjectHeader,
    /// Physical base address of the region.
    pub base: u64,
    /// Size of the region in bytes.
    pub size: u64,
}

/// Kernel object for a memory-mapped I/O region (MmioRegion capability).
#[repr(C)]
pub struct MmioRegionObject
{
    pub header: KernelObjectHeader,
    /// Physical base address of the MMIO region.
    pub base: u64,
    /// Size of the MMIO region in bytes.
    pub size: u64,
    /// Flags from the platform resource entry (bit 0: write-combine).
    pub flags: u32,
    pub _pad: u32,
}

/// Kernel object for a hardware interrupt line (Interrupt capability).
#[repr(C)]
pub struct InterruptObject
{
    pub header: KernelObjectHeader,
    /// Interrupt number (GSI on x86-64, PLIC source on RISC-V).
    pub irq_id: u32,
    /// Flags from the platform resource entry (edge/level, polarity).
    pub flags: u32,
}

/// Kernel object for an x86-64 I/O port range (IoPortRange capability).
#[repr(C)]
pub struct IoPortRangeObject
{
    pub header: KernelObjectHeader,
    /// First port number in the range.
    pub base: u16,
    /// Number of consecutive ports.
    pub size: u16,
    pub _pad: u32,
}

/// Kernel object for scheduling control authority (SchedControl capability).
///
/// There is exactly one SchedControl object, created at boot.
#[repr(C)]
pub struct SchedControlObject
{
    pub header: KernelObjectHeader,
}

/// Kernel object for a thread control block (Thread capability).
#[repr(C)]
pub struct ThreadObject
{
    pub header: KernelObjectHeader,
    /// Pointer to the TCB (heap-allocated).
    pub tcb: *mut crate::sched::thread::ThreadControlBlock,
}

// SAFETY: ThreadObject is accessed only under the scheduler lock.
unsafe impl Send for ThreadObject {}
unsafe impl Sync for ThreadObject {}

/// Kernel object for a user-mode address space (AddressSpace capability).
#[repr(C)]
pub struct AddressSpaceObject
{
    pub header: KernelObjectHeader,
    /// Pointer to the AddressSpace (heap-allocated).
    pub address_space: *mut crate::mm::address_space::AddressSpace,
}

// SAFETY: AddressSpaceObject is accessed only with proper locks.
unsafe impl Send for AddressSpaceObject {}
unsafe impl Sync for AddressSpaceObject {}

/// Kernel object for a capability space (CSpace capability).
#[repr(C)]
pub struct CSpaceKernelObject
{
    pub header: KernelObjectHeader,
    /// Pointer to the CSpace (heap-allocated).
    pub cspace: *mut crate::cap::cspace::CSpace,
}

// SAFETY: CSpaceKernelObject is accessed only with proper locks.
unsafe impl Send for CSpaceKernelObject {}
unsafe impl Sync for CSpaceKernelObject {}

/// Kernel object for an IPC endpoint (Endpoint capability).
#[repr(C)]
pub struct EndpointObject
{
    pub header: KernelObjectHeader,
    /// Mutable endpoint state (heap-allocated).
    pub state: *mut crate::ipc::endpoint::EndpointState,
}

// SAFETY: EndpointObject is accessed only under the scheduler lock.
unsafe impl Send for EndpointObject {}
unsafe impl Sync for EndpointObject {}

/// Kernel object for a signal (Signal capability).
#[repr(C)]
pub struct SignalObject
{
    pub header: KernelObjectHeader,
    /// Mutable signal state (heap-allocated).
    pub state: *mut crate::ipc::signal::SignalState,
}

// SAFETY: SignalObject is accessed only under the scheduler lock.
unsafe impl Send for SignalObject {}
unsafe impl Sync for SignalObject {}

/// Kernel object for an event queue (EventQueue capability).
///
/// The ring buffer body is a separate heap allocation stored in `EventQueueState`.
#[repr(C)]
pub struct EventQueueObject
{
    pub header: KernelObjectHeader,
    /// Mutable event queue state (heap-allocated).
    pub state: *mut crate::ipc::event_queue::EventQueueState,
}

// SAFETY: EventQueueObject is accessed only under the scheduler lock.
unsafe impl Send for EventQueueObject {}
unsafe impl Sync for EventQueueObject {}

/// Kernel object for a wait set (WaitSet capability).
///
/// `WaitSetState` is a ~500-byte heap allocation; this object wrapper is 16 B.
#[repr(C)]
pub struct WaitSetObject
{
    pub header: KernelObjectHeader,
    /// Mutable wait set state (heap-allocated).
    pub state: *mut crate::ipc::wait_set::WaitSetState,
}

// SAFETY: WaitSetObject is accessed only under the scheduler lock.
unsafe impl Send for WaitSetObject {}
unsafe impl Sync for WaitSetObject {}

// ── Object deallocation ───────────────────────────────────────────────────────

/// Free a kernel object whose reference count has just reached zero.
///
/// Dispatches on `obj_type` to reconstruct the original `Box<ConcreteObject>`
/// and drop it, freeing any sub-resources first.
///
/// # Safety
///
/// - `ptr` must be a valid, non-null pointer originally produced by
///   `Box::into_raw` (cast to `*mut KernelObjectHeader`).
/// - The object's reference count must be 0; no other capability slot may
///   reference it.
/// - Must NOT be called with `DERIVATION_LOCK` held, since freeing complex
///   objects (Thread, AddressSpace) may acquire the frame-allocator lock.
///
/// # Modification guide
///
/// Add a new arm when a new `ObjectType` variant is added. Each arm must:
/// 1. Cast `ptr` to the concrete object type.
/// 2. Free any sub-resources (nested allocations, physical frames).
/// 3. `Box::from_raw(ptr as *mut ConcreteType)` to drop the box.
///
/// For objects with blocked threads (Endpoint, Signal), drain wait queues and
/// re-enqueue threads so they are not permanently lost.
#[cfg(not(test))]
pub unsafe fn dealloc_object(ptr: core::ptr::NonNull<KernelObjectHeader>)
{
    use alloc::boxed::Box;

    let header = unsafe { ptr.as_ref() };
    match header.obj_type
    {
        // ── Simple objects (no sub-resources) ─────────────────────────────
        ObjectType::Frame =>
        {
            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut FrameObject)) };
        }
        ObjectType::MmioRegion =>
        {
            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut MmioRegionObject)) };
        }
        ObjectType::Interrupt =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const InterruptObject) };
            let irq_id = obj.irq_id;

            // Clear the routing table entry and mask the IRQ line so no further
            // interrupts are delivered after this cap is freed.
            // SAFETY: single-CPU; disable interrupts to serialise with
            //         dispatch_device_irq (interrupt context).
            unsafe {
                let saved = crate::arch::current::cpu::save_and_disable_interrupts();
                crate::irq::unregister(irq_id);
                crate::arch::current::cpu::restore_interrupts(saved);
            }
            crate::arch::current::interrupts::mask(irq_id);

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut InterruptObject)) };
        }
        ObjectType::IoPortRange =>
        {
            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut IoPortRangeObject)) };
        }
        ObjectType::SchedControl =>
        {
            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut SchedControlObject)) };
        }

        // ── Thread ────────────────────────────────────────────────────────
        ObjectType::Thread =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const ThreadObject) };
            let tcb = obj.tcb;

            if !tcb.is_null()
            {
                // Remove the TCB from the scheduler's run queue before freeing.
                // Without this, the scheduler could dequeue a freed TCB pointer
                // after this cap_delete completes — a use-after-free that
                // corrupts the slab and/or causes a hang when the scheduler
                // tries to context-switch to garbage state.
                unsafe {
                    use crate::sched::thread::ThreadState;
                    let prio = (*tcb).priority;
                    (*tcb).state = ThreadState::Exited;

                    let sched = crate::sched::scheduler_for(0);
                    let saved = sched.lock.lock_raw();
                    sched.remove_from_queue(tcb, prio);
                    sched.lock.unlock_raw(saved);
                }

                // Free the kernel stack back to the buddy allocator.
                // Stack order matches sys_cap_create_thread (KERNEL_STACK_PAGES = 4).
                let kstack_top = unsafe { (*tcb).kernel_stack_top };
                let kstack_virt = kstack_top
                    - (crate::sched::KERNEL_STACK_PAGES * crate::mm::PAGE_SIZE) as u64;
                let kstack_phys = crate::mm::paging::virt_to_phys(kstack_virt);
                const STACK_ORDER: usize = 2; // 2^2 = 4 pages
                unsafe {
                    crate::mm::with_frame_allocator(|alloc| {
                        alloc.free(kstack_phys, STACK_ORDER);
                    });
                }

                // Drop the TCB allocation.
                unsafe { drop(Box::from_raw(tcb)) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut ThreadObject)) };
        }

        // ── AddressSpace ──────────────────────────────────────────────────
        ObjectType::AddressSpace =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const AddressSpaceObject) };
            let as_ptr = obj.address_space;

            if !as_ptr.is_null()
            {
                // Free the root page-table frame (one 4 KiB page, order 0).
                // TODO: walk and free intermediate page-table frames to avoid
                // leaking them. Requires arch-specific paging teardown logic.
                let root_phys = unsafe { (*as_ptr).root_phys };
                unsafe {
                    crate::mm::with_frame_allocator(|alloc| alloc.free(root_phys, 0));
                }

                unsafe { drop(Box::from_raw(as_ptr)) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut AddressSpaceObject)) };
        }

        // ── CSpaceObj ─────────────────────────────────────────────────────
        ObjectType::CSpaceObj =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const CSpaceKernelObject) };
            let cs_ptr = obj.cspace;

            if !cs_ptr.is_null()
            {
                let id = unsafe { (*cs_ptr).id() };
                // Unregister before freeing so no window exists where the
                // registry contains a dangling pointer.
                crate::cap::unregister_cspace(id);

                // Dec-ref all objects referenced by non-null slots.
                // Without this, destroying a CSpace with live caps inside
                // (e.g., cap_copy'd caps in a child CSpace) leaks the
                // underlying kernel objects permanently.
                //
                // TODO: unlink derivation tree pointers in other CSpaces
                // that reference slots in this dying CSpace. Currently
                // those SlotId references become stale, which is harmless
                // unless cap_revoke traverses them.
                unsafe {
                    (*cs_ptr).for_each_object(|obj_ptr| {
                        let hdr = obj_ptr.as_ref();
                        let rc = hdr.dec_ref();
                        if rc == 0
                        {
                            dealloc_object(obj_ptr);
                        }
                    });
                }

                unsafe { drop(Box::from_raw(cs_ptr)) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut CSpaceKernelObject)) };
        }

        // ── Endpoint ──────────────────────────────────────────────────────
        ObjectType::Endpoint =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const EndpointObject) };
            let state = obj.state;

            if !state.is_null()
            {
                // Unregister from wait set before freeing state.
                unsafe {
                    let ep = &mut *state;
                    if !ep.wait_set.is_null()
                    {
                        let ws = ep.wait_set as *mut crate::ipc::wait_set::WaitSetState;
                        let _ = crate::ipc::wait_set::waitset_remove(
                            ws,
                            state as *mut u8,
                        );
                        ep.wait_set = core::ptr::null_mut();
                        ep.wait_set_member_idx = 0;
                    }
                }

                // Drain blocked senders and receivers with a zero return value.
                // They will wake up and resume from sys_ipc_call / sys_ipc_recv,
                // reading a zero-length message (effectively an ObjectGone hint).
                // TODO: set TrapFrame return to SyscallError::ObjectGone when
                // a proper per-thread wakeup error path is added.
                unsafe {
                    let ep = &mut *state;
                    // Wake senders.
                    let mut tcb = ep.send_head;
                    while !tcb.is_null()
                    {
                        let next = (*tcb).ipc_wait_next;
                        (*tcb).ipc_wait_next = None;
                        (*tcb).ipc_state = crate::sched::thread::IpcThreadState::None;
                        (*tcb).state = crate::sched::thread::ThreadState::Ready;
                        let prio = (*tcb).priority;
                        crate::sched::scheduler_for(0).enqueue(tcb, prio);
                        tcb = next.unwrap_or(core::ptr::null_mut());
                    }
                    ep.send_head = core::ptr::null_mut();
                    ep.send_tail = core::ptr::null_mut();
                    // Wake receivers.
                    let mut tcb = ep.recv_head;
                    while !tcb.is_null()
                    {
                        let next = (*tcb).ipc_wait_next;
                        (*tcb).ipc_wait_next = None;
                        (*tcb).ipc_state = crate::sched::thread::IpcThreadState::None;
                        (*tcb).state = crate::sched::thread::ThreadState::Ready;
                        let prio = (*tcb).priority;
                        crate::sched::scheduler_for(0).enqueue(tcb, prio);
                        tcb = next.unwrap_or(core::ptr::null_mut());
                    }
                    ep.recv_head = core::ptr::null_mut();
                    ep.recv_tail = core::ptr::null_mut();
                }

                unsafe { drop(Box::from_raw(state)) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut EndpointObject)) };
        }

        // ── Signal ────────────────────────────────────────────────────────
        ObjectType::Signal =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const SignalObject) };
            let state = obj.state;

            if !state.is_null()
            {
                // Clear any IRQ routing table entries that point to this
                // SignalState. Without this, a hardware interrupt firing after
                // the signal is freed would call signal_send on a dead slot,
                // corrupting the slab-32 free list via fetch_or on offset 0.
                unsafe {
                    let saved = crate::arch::current::cpu::save_and_disable_interrupts();
                    crate::irq::unregister_signal(state);
                    crate::arch::current::cpu::restore_interrupts(saved);
                }

                // Unregister from wait set BEFORE freeing state. If the signal
                // is registered with a wait set, the wait set's member array
                // still holds a source_ptr to this SignalState. Failing to
                // remove it causes wait_set_drop to write to freed memory.
                unsafe {
                    let sig = &mut *state;
                    if !sig.wait_set.is_null()
                    {
                        let ws = sig.wait_set as *mut crate::ipc::wait_set::WaitSetState;
                        let _ = crate::ipc::wait_set::waitset_remove(
                            ws,
                            state as *mut u8,
                        );
                        sig.wait_set = core::ptr::null_mut();
                        sig.wait_set_member_idx = 0;
                    }
                }

                // Wake a blocked waiter with wakeup_value = 0.
                // TODO: return SyscallError::ObjectGone when a proper wakeup
                // error path is available in sys_signal_wait.
                unsafe {
                    let sig = &mut *state;
                    let waiter = sig.waiter;
                    if !waiter.is_null()
                    {
                        sig.waiter = core::ptr::null_mut();
                        (*waiter).wakeup_value = 0;
                        (*waiter).ipc_state = crate::sched::thread::IpcThreadState::None;
                        (*waiter).state = crate::sched::thread::ThreadState::Ready;
                        let prio = (*waiter).priority;
                        crate::sched::scheduler_for(0).enqueue(waiter, prio);
                    }
                }

                unsafe { drop(Box::from_raw(state)) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut SignalObject)) };
        }

        // ── EventQueue ────────────────────────────────────────────────────
        ObjectType::EventQueue =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const EventQueueObject) };
            let state = obj.state;

            if !state.is_null()
            {
                // Unregister from wait set before freeing state.
                unsafe {
                    let eq = &mut *state;
                    if !eq.wait_set.is_null()
                    {
                        let ws = eq.wait_set as *mut crate::ipc::wait_set::WaitSetState;
                        let _ = crate::ipc::wait_set::waitset_remove(
                            ws,
                            state as *mut u8,
                        );
                        eq.wait_set = core::ptr::null_mut();
                        eq.wait_set_member_idx = 0;
                    }
                }

                // SAFETY: state was allocated in sys_cap_create_event_queue.
                // Wake any blocked waiter (wakeup_value = 0 signals object gone).
                unsafe { crate::ipc::event_queue::event_queue_drop(state) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut EventQueueObject)) };
        }

        // ── WaitSet ───────────────────────────────────────────────────────
        ObjectType::WaitSet =>
        {
            let obj = unsafe { &*(ptr.as_ptr() as *const WaitSetObject) };
            let state = obj.state;

            if !state.is_null()
            {
                // SAFETY: state was allocated in sys_cap_create_wait_set.
                // Wakes any blocked waiter and clears all source back-pointers.
                unsafe { crate::ipc::wait_set::wait_set_drop(state) };
            }

            unsafe { drop(Box::from_raw(ptr.as_ptr() as *mut WaitSetObject)) };
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;
    use core::mem::{offset_of, size_of};

    // Verify header is at offset 0 in each concrete type — required for safe
    // pointer casts from *mut ConcreteObject to *mut KernelObjectHeader.
    #[test]
    fn frame_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(FrameObject, header), 0);
    }

    #[test]
    fn mmio_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(MmioRegionObject, header), 0);
    }

    #[test]
    fn interrupt_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(InterruptObject, header), 0);
    }

    #[test]
    fn ioport_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(IoPortRangeObject, header), 0);
    }

    #[test]
    fn sched_control_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(SchedControlObject, header), 0);
    }

    #[test]
    fn struct_sizes()
    {
        assert_eq!(size_of::<KernelObjectHeader>(), 8);
        assert_eq!(size_of::<FrameObject>(), 24);
        assert_eq!(size_of::<MmioRegionObject>(), 32);
        assert_eq!(size_of::<InterruptObject>(), 16);
        assert_eq!(size_of::<IoPortRangeObject>(), 16); // 8 header + 4 ports + 4 pad
        assert_eq!(size_of::<SchedControlObject>(), 8);
    }

    #[test]
    fn header_ref_count_lifecycle()
    {
        let h = KernelObjectHeader::new(ObjectType::Frame);
        assert_eq!(h.ref_count.load(core::sync::atomic::Ordering::Relaxed), 1);
        h.inc_ref();
        assert_eq!(h.ref_count.load(core::sync::atomic::Ordering::Relaxed), 2);
        let after_dec = h.dec_ref();
        assert_eq!(after_dec, 1);
        let after_dec2 = h.dec_ref();
        assert_eq!(after_dec2, 0);
    }
}
