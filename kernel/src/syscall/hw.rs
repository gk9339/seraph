// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/hw.rs

//! Hardware access syscall handlers.
//!
//! Implements:
//! - `SYS_IRQ_ACK` (29): re-enable a masked interrupt line.
//! - `SYS_IRQ_REGISTER` (30): bind a Signal to an interrupt line.
//! - `SYS_MMIO_MAP` (34): map an MMIO region into an address space.
//! - `SYS_IOPORT_BIND` (35): bind an I/O port range to a thread (`x86_64` only).
//! - `SYS_DMA_GRANT` (36): return a frame's physical address for DMA use
//!   (no-IOMMU fallback; requires `FLAG_DMA_UNSAFE`).
//!
//! # Adding new hardware syscalls
//! 1. Add a new `pub fn sys_hw_*` in this file.
//! 2. Import the constant in `syscall/mod.rs`.
//! 3. Add a dispatch arm in `syscall/mod.rs`.
//! 4. Add a userspace wrapper in `shared/syscall/src/lib.rs`.

// cast_possible_truncation: capability slot indices extracted from 64-bit trap frame
// registers are always u32-range values. Seraph runs on 64-bit only; no truncation occurs.
#![allow(clippy::cast_possible_truncation)]

use crate::arch::current::trap_frame::TrapFrame;
use syscall::SyscallError;

// ── SYS_IRQ_ACK ───────────────────────────────────────────────────────────────

/// `SYS_IRQ_ACK` (29): re-enable an interrupt line after the driver has handled
/// the interrupt.
///
/// arg0 = Interrupt cap index (must have SIGNAL right).
///
/// Unmasks the IRQ line at the interrupt controller, allowing the next
/// interrupt delivery. The driver must call this after clearing the interrupt
/// source in the device to avoid an interrupt storm.
///
/// Returns 0 on success.
#[cfg(not(test))]
pub fn sys_irq_ack(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::InterruptObject;
    use crate::cap::slot::{CapTag, Rights};
    use crate::syscall::current_tcb;

    let irq_cap_idx = tf.arg(0) as u32;

    // SAFETY: current_tcb() returns current thread; interrupt context ensures it is set.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: tcb validated non-null; cspace set at thread creation.
    let cspace = unsafe { (*tcb).cspace };

    // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
    let irq_slot =
        unsafe { super::lookup_cap(cspace, irq_cap_idx, CapTag::Interrupt, Rights::SIGNAL) }?;
    let irq_id = {
        let obj = irq_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed Interrupt; object was allocated as Box<InterruptObject>.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe { (*obj.as_ptr().cast::<InterruptObject>()).irq_id }
    };

    // Unmask at the interrupt controller to re-enable delivery.
    crate::arch::current::interrupts::unmask(irq_id);

    Ok(0)
}

#[cfg(test)]
pub fn sys_irq_ack(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_IRQ_REGISTER ─────────────────────────────────────────────────────────

/// `SYS_IRQ_REGISTER` (30): bind a Signal to an interrupt line.
///
/// arg0 = Interrupt cap index (must have SIGNAL right).
/// arg1 = Signal cap index (must have SIGNAL right).
///
/// When the interrupt fires:
/// 1. The IRQ is masked at the controller.
/// 2. Bit 0 is `ORed` into the Signal.
/// 3. Any thread blocked on `SYS_SIGNAL_WAIT` for this signal is woken.
/// 4. The driver must call `SYS_IRQ_ACK` to re-enable delivery.
///
/// On `x86_64`: programs the IOAPIC redirection entry (masked until first ACK).
/// On RISC-V: enables the PLIC source (masked at controller until first ACK).
///
/// Returns 0 on success.
#[cfg(not(test))]
pub fn sys_irq_register(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::{InterruptObject, SignalObject};
    use crate::cap::slot::{CapTag, Rights};
    use crate::syscall::current_tcb;

    let irq_cap_idx = tf.arg(0) as u32;
    let sig_cap_idx = tf.arg(1) as u32;

    // SAFETY: current_tcb() returns current thread; interrupt context ensures it is set.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: tcb validated non-null; cspace set at thread creation.
    let cspace = unsafe { (*tcb).cspace };

    // Resolve Interrupt cap.
    // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
    let irq_slot =
        unsafe { super::lookup_cap(cspace, irq_cap_idx, CapTag::Interrupt, Rights::SIGNAL) }?;
    let irq_id = {
        let obj = irq_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed Interrupt; object was allocated as Box<InterruptObject>.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe { (*obj.as_ptr().cast::<InterruptObject>()).irq_id }
    };

    // Resolve Signal cap.
    // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
    let sig_slot =
        unsafe { super::lookup_cap(cspace, sig_cap_idx, CapTag::Signal, Rights::SIGNAL) }?;
    let sig_state = {
        let obj = sig_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed Signal; object was allocated as Box<SignalObject>.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe { (*obj.as_ptr().cast::<SignalObject>()).state }
    };

    // Register the signal in the IRQ routing table.
    // Disable interrupts to serialise with dispatch_device_irq, which reads
    // the table from interrupt context.
    // SAFETY: save_and_disable_interrupts/restore_interrupts are paired;
    //         irq::register requires interrupts disabled.
    unsafe {
        let saved = crate::arch::current::cpu::save_and_disable_interrupts();
        crate::irq::register(irq_id, sig_state);
        crate::arch::current::cpu::restore_interrupts(saved);
    }

    // Program arch-specific interrupt routing.
    // x86_64: write IOAPIC redirection entry (entry starts masked; driver ACKs
    //         to unmask after registering).
    // RISC-V: enable the PLIC source (starts masked at controller; ACK unmasks).
    #[cfg(target_arch = "x86_64")]
    // SAFETY: irq_id validated from capability; route() requires valid IRQ number.
    unsafe {
        crate::arch::current::ioapic::route(
            irq_id,
            crate::arch::current::ioapic::DEVICE_VECTOR_BASE + irq_id as u8,
        );
        // Entry is left masked by route(); driver unmasks via SYS_IRQ_ACK.
    }

    #[cfg(target_arch = "riscv64")]
    {
        // Enable at PLIC, then immediately mask until the driver ACKs.
        crate::arch::current::interrupts::plic_enable(irq_id);
        crate::arch::current::interrupts::mask(irq_id);
    }

    Ok(0)
}

#[cfg(test)]
pub fn sys_irq_register(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_MMIO_MAP ─────────────────────────────────────────────────────────────

/// `SYS_MMIO_MAP` (34): map an MMIO region into a user address space.
///
/// arg0 = `AddressSpace` cap index (must have MAP right).
/// arg1 = `MmioRegion` cap index (must have MAP right).
/// arg2 = virtual base address (page-aligned, user half).
/// arg3 = flags (bit 1 = WRITE; executable mappings are always rejected).
///
/// All pages are mapped with `uncacheable = true` (PCD|PWT on `x86_64`,
/// no-op on RISC-V QEMU — see [`PageFlags`] comment).
///
/// Returns 0 on success.
#[cfg(not(test))]
pub fn sys_mmio_map(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::{AddressSpaceObject, MmioRegionObject};
    use crate::cap::slot::{CapTag, Rights};
    use crate::mm::paging::PageFlags;
    use crate::mm::{with_frame_allocator, PAGE_SIZE};
    use crate::syscall::current_tcb;

    // Virtual address must be page-aligned and in user half.
    const USER_HALF_TOP: u64 = 0x0000_8000_0000_0000;
    let aspace_idx = tf.arg(0) as u32;
    let mmio_idx = tf.arg(1) as u32;

    let virt_base = tf.arg(2);
    let flags = tf.arg(3);
    if virt_base & 0xFFF != 0 || virt_base >= USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }

    // SAFETY: current_tcb() returns current thread; interrupt context ensures it is set.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: tcb validated non-null; cspace set at thread creation.
    let cspace = unsafe { (*tcb).cspace };

    // Resolve MmioRegion cap.
    // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
    let mmio_slot =
        unsafe { super::lookup_cap(cspace, mmio_idx, CapTag::MmioRegion, Rights::MAP) }?;
    let (mmio_phys, mmio_size) = {
        let obj = mmio_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed MmioRegion; object was allocated as Box<MmioRegionObject>.
        #[allow(clippy::cast_ptr_alignment)]
        let mo = unsafe { &*obj.as_ptr().cast::<MmioRegionObject>() };
        (mo.base, mo.size)
    };

    // Size must be at least one page and page-aligned.
    if mmio_size == 0 || mmio_size & 0xFFF != 0
    {
        return Err(SyscallError::InvalidArgument);
    }
    let page_count = (mmio_size / PAGE_SIZE as u64) as usize;

    // Guard against virtual range overflow.
    let virt_end = virt_base
        .checked_add(mmio_size)
        .ok_or(SyscallError::InvalidArgument)?;
    if virt_end > USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }

    // Resolve AddressSpace cap.
    // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
    let as_slot =
        unsafe { super::lookup_cap(cspace, aspace_idx, CapTag::AddressSpace, Rights::MAP) }?;
    let as_ptr = {
        let obj = as_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed AddressSpace; object was allocated as Box<AddressSpaceObject>.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe { (*obj.as_ptr().cast::<AddressSpaceObject>()).address_space }
    };

    // MMIO mappings are never executable.
    // Writability is derived from the cap's WRITE right, not the flags arg.
    // The flags arg is reserved for future use (cache mode overrides, etc.).
    let writable = mmio_slot.rights.contains(Rights::WRITE);
    let _ = flags; // reserved
    let page_flags = PageFlags {
        readable: true,
        writable,
        executable: false,
        uncacheable: true,
    };

    // Map each page.
    for i in 0..page_count
    {
        let virt = virt_base + (i * PAGE_SIZE) as u64;
        let phys = mmio_phys + (i * PAGE_SIZE) as u64;

        let result = with_frame_allocator(|alloc| {
            // SAFETY: virt in user range (validated above); phys from a
            // kernel-provisioned MmioRegion boot object.
            unsafe { (*as_ptr).map_page(virt, phys, page_flags, alloc) }
        });

        result.map_err(|()| SyscallError::OutOfMemory)?;
    }

    Ok(0)
}

#[cfg(test)]
pub fn sys_mmio_map(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_IOPORT_BIND ──────────────────────────────────────────────────────────

/// `SYS_IOPORT_BIND` (35): grant a thread access to an I/O port range.
///
/// arg0 = Thread cap index (must have CONTROL right).
/// arg1 = `IoPortRange` cap index (must have USE right).
///
/// On first bind, a 8 KiB per-thread IOPB bitmap is heap-allocated and all
/// ports are denied (0xFF). The requested range bits are then cleared (0 =
/// allowed). On context switch the bitmap is copied into the TSS IOPB region.
///
/// On RISC-V: always returns `NotSupported` (no I/O port concept).
///
/// Returns 0 on success.
// needless_return: the cfg-gated early return is required to terminate the
// riscv64 path; the x86_64 path follows in the same function body.
#[cfg(not(test))]
#[allow(clippy::needless_return)]
pub fn sys_ioport_bind(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // RISC-V has no I/O port space.
    #[cfg(target_arch = "riscv64")]
    {
        let _ = tf;
        // unnecessary_wraps suppressed: sys_ioport_bind must match the dispatch
        // table signature Result<u64, SyscallError> on all targets.
        return Err(SyscallError::NotSupported);
    }

    #[cfg(target_arch = "x86_64")]
    {
        use crate::arch::current::gdt;
        use crate::cap::object::{IoPortRangeObject, ThreadObject};
        use crate::cap::slot::{CapTag, Rights};
        use crate::syscall::current_tcb;

        let thread_idx = tf.arg(0) as u32;
        let ioport_idx = tf.arg(1) as u32;

        // SAFETY: current_tcb() returns current thread; interrupt context ensures it is set.
        let caller_tcb = unsafe { current_tcb() };
        if caller_tcb.is_null()
        {
            return Err(SyscallError::InvalidCapability);
        }
        // SAFETY: tcb validated non-null; cspace set at thread creation.
        let cspace = unsafe { (*caller_tcb).cspace };

        // Resolve Thread cap.
        // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
        let th_slot =
            unsafe { super::lookup_cap(cspace, thread_idx, CapTag::Thread, Rights::CONTROL) }?;
        let target_tcb = {
            let obj = th_slot.object.ok_or(SyscallError::InvalidCapability)?;
            // SAFETY: tag confirmed Thread; object was allocated as Box<ThreadObject>.
            #[allow(clippy::cast_ptr_alignment)]
            unsafe { (*obj.as_ptr().cast::<ThreadObject>()).tcb }
        };
        if target_tcb.is_null()
        {
            return Err(SyscallError::InvalidCapability);
        }

        // Resolve IoPortRange cap.
        // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
        let port_slot =
            unsafe { super::lookup_cap(cspace, ioport_idx, CapTag::IoPortRange, Rights::USE) }?;
        let (port_base, port_size) = {
            let obj = port_slot.object.ok_or(SyscallError::InvalidCapability)?;
            // SAFETY: tag confirmed IoPortRange; object was allocated as Box<IoPortRangeObject>.
            #[allow(clippy::cast_ptr_alignment)]
            let po = unsafe { &*obj.as_ptr().cast::<IoPortRangeObject>() };
            (po.base, po.size)
        };

        // Allocate per-thread IOPB on first bind.
        // SAFETY: target_tcb validated non-null; iopb field always valid.
        if unsafe { (*target_tcb).iopb.is_null() }
        {
            let bitmap = alloc::boxed::Box::new([0xFFu8; gdt::IOPB_SIZE]);
            // SAFETY: target_tcb validated non-null; iopb field owned by TCB.
            unsafe {
                (*target_tcb).iopb = alloc::boxed::Box::into_raw(bitmap);
            }
        }

        // Clear the bits for the requested port range (0 = allow).
        // SAFETY: iopb is non-null after the allocation above; target_tcb validated.
        unsafe {
            gdt::permit_port_range(&mut *(*target_tcb).iopb, port_base, port_size);
        }

        // If binding to the currently running thread, reload the TSS IOPB
        // immediately so in/out instructions work without a context switch.
        if target_tcb == caller_tcb
        {
            // SAFETY: iopb non-null after allocation; target_tcb validated.
            unsafe {
                gdt::load_iopb(Some(&*(*target_tcb).iopb));
            }
        }

        Ok(0)
    }
}

#[cfg(test)]
pub fn sys_ioport_bind(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_DMA_GRANT ────────────────────────────────────────────────────────────

/// `SYS_DMA_GRANT` (36): return the physical address of a frame for DMA use.
///
/// arg0 = Frame cap index (must have MAP right).
/// arg1 = `device_id` (reserved; unused in no-IOMMU path).
/// arg2 = flags (must include `FLAG_DMA_UNSAFE` when no IOMMU is present).
///
/// Without an IOMMU, the DMA transfer is not hardware-isolated: the device
/// can access the full physical frame. The caller must set `FLAG_DMA_UNSAFE`
/// to acknowledge this and accept the security implications. If the flag is
/// absent, `DmaUnsafe` is returned instead of the physical address.
///
/// Returns the physical base address of the frame on success.
///
// TODO: When an IOMMU driver is present, program the device's
// second-level page table instead of returning the raw physical address.
// FLAG_DMA_UNSAFE is then only checked for devices without an active IOMMU
// domain. See also: IOMMU grant revocation (track active DMA grants per
// frame for teardown on cap revocation).
#[cfg(not(test))]
pub fn sys_dma_grant(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::FrameObject;
    use crate::cap::slot::{CapTag, Rights};
    use crate::syscall::current_tcb;
    use syscall::FLAG_DMA_UNSAFE;

    let frame_idx = tf.arg(0) as u32;
    // arg1 = device_id: reserved for future IOMMU domain lookup; unused now.
    let flags = tf.arg(2);

    // SAFETY: current_tcb() returns current thread; interrupt context ensures it is set.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: tcb validated non-null; cspace set at thread creation.
    let cspace = unsafe { (*tcb).cspace };

    // SAFETY: caller_cspace validated; lookup_cap checks tag and rights.
    let frame_slot = unsafe { super::lookup_cap(cspace, frame_idx, CapTag::Frame, Rights::MAP) }?;
    let frame_phys = {
        let obj = frame_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed Frame; object was allocated as Box<FrameObject>.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe { (*obj.as_ptr().cast::<FrameObject>()).base }
    };

    // No IOMMU present: require explicit unsafe acknowledgment.
    if flags & FLAG_DMA_UNSAFE == 0
    {
        return Err(SyscallError::DmaUnsafe);
    }

    Ok(frame_phys)
}

#[cfg(test)]
pub fn sys_dma_grant(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}
