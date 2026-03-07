// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/gdt.rs

//! Global Descriptor Table and Task State Segment.
//!
//! Sets up a minimal flat GDT for 64-bit kernel operation:
//! - Descriptor 0x00: null
//! - Descriptor 0x08: kernel CS (64-bit code, ring 0)
//! - Descriptor 0x10: kernel DS (data, ring 0)
//! - Descriptor 0x18: user DS  (data, ring 3)
//! - Descriptor 0x20: user CS  (64-bit code, ring 3)
//! - Descriptor 0x28: TSS low 64 bits  (first half of 128-bit system descriptor)
//! - Descriptor 0x30: TSS high 64 bits (second half)
//!
//! User DS appears before user CS because SYSRET loads:
//!   CS = STAR[63:48]+16, SS = STAR[63:48]+8
//! so the layout `user_ds(0x18), user_cs(0x20)` is required for SYSRET.
//!
//! IST stacks for double-fault (IST1) and NMI (IST2) are passed in by the
//! caller (allocated from the heap via `Box::leak`).
//!
//! # Modification notes
//! - To add a new ring-3 segment: insert it after user_cs and before the TSS
//!   pair; update the selector constants accordingly.
//! - To add more IST stacks: add IST fields to the TSS, pass the stack tops
//!   into `init`, and reference them in IDT gate descriptors.

// Unit tests exercise descriptor encoding only; hardware ops are #[cfg(not(test))].

// ── Selector constants ────────────────────────────────────────────────────────

/// Kernel code segment selector (ring 0, GDT index 1, RPL=0).
pub const KERNEL_CS: u16 = 0x08;
/// Kernel data segment selector (ring 0, GDT index 2, RPL=0).
pub const KERNEL_DS: u16 = 0x10;
/// User data segment selector (ring 3, GDT index 3, RPL=3).
pub const USER_DS: u16 = 0x1B;
/// User code segment selector (ring 3, GDT index 4, RPL=3).
pub const USER_CS: u16 = 0x23;
/// TSS selector (GDT index 5, RPL=0). 128-bit descriptor spans 0x28–0x37.
pub const TSS_SEL: u16 = 0x28;

// ── GDT and TSS storage ───────────────────────────────────────────────────────

/// 64-bit Task State Segment — see Intel SDM Vol 3A §8.2.1.
///
/// `repr(C, packed)` is required: the hardware layout is fixed.
/// Size must be exactly 104 bytes.
///
/// Only fields used in Phase 5 are named; reserved fields are prefixed `_`.
#[repr(C, packed)]
pub struct Tss
{
    _reserved0: u32,
    /// Ring-0 stack pointer (loaded by the CPU on ring-3 → ring-0 transitions).
    pub rsp0: u64,
    _rsp1: u64,
    _rsp2: u64,
    _reserved1: u64,
    /// IST1 — used by the double-fault gate.
    pub ist1: u64,
    /// IST2 — used by the NMI gate.
    pub ist2: u64,
    _ist3: u64,
    _ist4: u64,
    _ist5: u64,
    _ist6: u64,
    _ist7: u64,
    _reserved2: u64,
    _reserved3: u16,
    /// I/O permission bitmap offset (104 = no IOPB).
    pub iopb_offset: u16,
}

/// The TSS, in BSS so it is zero-initialised.
///
/// `static mut` is only written during single-threaded boot init.
#[cfg(not(test))]
static mut TSS: Tss = Tss {
    _reserved0: 0,
    rsp0: 0,
    _rsp1: 0,
    _rsp2: 0,
    _reserved1: 0,
    ist1: 0,
    ist2: 0,
    _ist3: 0,
    _ist4: 0,
    _ist5: 0,
    _ist6: 0,
    _ist7: 0,
    _reserved2: 0,
    _reserved3: 0,
    iopb_offset: 104,
};

/// GDT: 7 × 64-bit descriptors, in BSS.
#[cfg(not(test))]
static mut GDT: [u64; 7] = [0u64; 7];

// ── GDTR ──────────────────────────────────────────────────────────────────────

/// Hardware GDTR format: limit (16-bit) followed immediately by base (64-bit).
///
/// Must be `packed` so there is no padding between `limit` and `base`.
#[repr(C, packed)]
struct Gdtr
{
    limit: u16,
    base: u64,
}

// ── Descriptor encoding ───────────────────────────────────────────────────────

/// Build a 64-bit long-mode code segment descriptor at `dpl` privilege level.
///
/// In 64-bit mode only P, DPL, S, type, and L matter:
/// - P=1: present
/// - S=1: code/data (not system)
/// - type=0b1010: execute/read, non-conforming
/// - L=1: 64-bit code segment
pub fn code_desc_64(dpl: u8) -> u64
{
    (1u64 << 47) // P
        | ((dpl as u64 & 3) << 45) // DPL
        | (1u64 << 44) // S
        | (0b1010u64 << 40) // type: execute+read
        | (1u64 << 53) // L
}

/// Build a 64-bit data segment descriptor at `dpl` privilege level.
///
/// In 64-bit mode data segments are nearly ignored but must be present.
/// - P=1, S=1, type=0b0010: data, writable
pub fn data_desc_64(dpl: u8) -> u64
{
    (1u64 << 47) // P
        | ((dpl as u64 & 3) << 45) // DPL
        | (1u64 << 44) // S
        | (0b0010u64 << 40) // type: data+writable
}

/// Encode the 128-bit TSS descriptor, split into two consecutive GDT entries.
///
/// Returns `(low, high)` where `low` goes into GDT\[5\] (0x28) and `high`
/// into GDT\[6\] (0x30).
///
/// Encoding:
/// - limit = sizeof(Tss)−1 = 103
/// - type = 0x89: P=1, DPL=0, 64-bit TSS available
pub fn tss_desc(tss_addr: u64) -> (u64, u64)
{
    let limit: u64 = (core::mem::size_of::<Tss>() as u64) - 1; // 103 = 0x67

    let low: u64 = (limit & 0xFFFF) // [15:0]  limit[15:0]
        | ((tss_addr & 0x00FF_FFFF) << 16) // [39:16] base[23:0]
        | (0x89u64 << 40) // [47:40] P+type (TSS available)
        | (((limit >> 16) & 0xF) << 48) // [51:48] limit[19:16]
        | (((tss_addr >> 24) & 0xFF) << 56); // [63:56] base[31:24]

    let high: u64 = tss_addr >> 32; // [31:0]  base[63:32]

    (low, high)
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Populate and load the GDT and TSS, then reload all segment registers.
///
/// Called once during boot. Parameters:
/// - `kernel_stack_top`: initial RSP0 (ring-0 stack top for TSS).
/// - `ist1_top`: top of the 8 KiB double-fault IST stack.
/// - `ist2_top`: top of the 8 KiB NMI IST stack.
///
/// # Safety
/// Must execute at ring 0 from a single-threaded context. The `GDT` and `TSS`
/// statics must not be accessed concurrently.
#[cfg(not(test))]
pub unsafe fn init(kernel_stack_top: u64, ist1_top: u64, ist2_top: u64)
{
    // SAFETY: single-threaded boot — exclusive access guaranteed.
    let gdt = unsafe { &mut *core::ptr::addr_of_mut!(GDT) };
    let tss = unsafe { &mut *core::ptr::addr_of_mut!(TSS) };

    // Configure TSS.
    tss.rsp0 = kernel_stack_top;
    tss.ist1 = ist1_top;
    tss.ist2 = ist2_top;
    tss.iopb_offset = 104;

    // Fill GDT slots.
    gdt[0] = 0; // null
    gdt[1] = code_desc_64(0); // 0x08 kernel CS
    gdt[2] = data_desc_64(0); // 0x10 kernel DS
    gdt[3] = data_desc_64(3); // 0x18 user DS (RPL=3 via selector constant)
    gdt[4] = code_desc_64(3); // 0x20 user CS (RPL=3 via selector constant)
    let tss_addr = core::ptr::addr_of!(*tss) as u64;
    let (tss_lo, tss_hi) = tss_desc(tss_addr);
    gdt[5] = tss_lo; // 0x28 TSS low
    gdt[6] = tss_hi; // 0x30 TSS high

    // Load GDTR.
    let gdtr = Gdtr {
        limit: (core::mem::size_of_val(gdt) - 1) as u16,
        base: gdt.as_ptr() as u64,
    };
    // SAFETY: gdtr is live on this stack frame.
    unsafe {
        core::arch::asm!(
            "lgdt [{0}]",
            in(reg) &gdtr,
            options(readonly, nostack, preserves_flags),
        );
    }

    // Reload CS by performing a far return into the kernel code segment.
    // This flushes the CPU's segment cache for CS.
    unsafe {
        core::arch::asm!(
            "push {cs}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            cs  = in(reg) KERNEL_CS as u64,
            tmp = lateout(reg) _,
            options(nostack),
        );
    }

    // Reload data/stack segment registers.
    unsafe {
        core::arch::asm!(
            "mov ds, {ds:x}",
            "mov es, {ds:x}",
            "mov ss, {ds:x}",
            "xor {z:e}, {z:e}",
            "mov fs, {z:x}",
            "mov gs, {z:x}",
            ds = in(reg) KERNEL_DS,
            z  = lateout(reg) _,
            options(nostack, nomem),
        );
    }

    // Load the TSS selector.
    unsafe {
        core::arch::asm!(
            "ltr {0:x}",
            in(reg) TSS_SEL,
            options(nostack, nomem),
        );
    }
}

/// Update the ring-0 RSP stored in the TSS.
///
/// Call before returning to ring-3 to ensure the correct kernel stack is
/// used on the next ring-3 → ring-0 transition.
///
/// # Safety
/// Must be called at ring 0. `init()` must have been called first.
#[cfg(not(test))]
pub unsafe fn set_rsp0(stack_top: u64)
{
    // SAFETY: caller holds any necessary lock; boot is single-threaded.
    unsafe {
        (*core::ptr::addr_of_mut!(TSS)).rsp0 = stack_top;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn tss_is_104_bytes()
    {
        assert_eq!(core::mem::size_of::<Tss>(), 104);
    }

    #[test]
    fn kernel_cs_is_64bit_code_ring0()
    {
        let d = code_desc_64(0);
        assert!(d & (1 << 47) != 0, "P not set");
        assert!(d & (1 << 44) != 0, "S not set");
        assert!(d & (1 << 53) != 0, "L not set");
        assert_eq!((d >> 45) & 3, 0, "DPL should be 0");
    }

    #[test]
    fn user_cs_is_64bit_code_ring3()
    {
        let d = code_desc_64(3);
        assert!(d & (1 << 47) != 0, "P not set");
        assert!(d & (1 << 53) != 0, "L not set");
        assert_eq!((d >> 45) & 3, 3, "DPL should be 3");
    }

    #[test]
    fn kernel_ds_is_data_ring0_l_clear()
    {
        let d = data_desc_64(0);
        assert!(d & (1 << 47) != 0, "P not set");
        assert_eq!((d >> 45) & 3, 0, "DPL should be 0");
        assert_eq!(d & (1 << 53), 0, "L must be 0 for data");
    }

    #[test]
    fn user_ds_is_data_ring3()
    {
        let d = data_desc_64(3);
        assert_eq!((d >> 45) & 3, 3, "DPL should be 3");
    }

    #[test]
    fn tss_desc_limit_and_type()
    {
        let (lo, _hi) = tss_desc(0xDEAD_BEEF_1234_5678);
        // limit[15:0] = 103
        assert_eq!(lo & 0xFFFF, 103, "limit low should be 103");
        // type byte = 0x89
        assert_eq!((lo >> 40) & 0xFF, 0x89, "type field");
    }

    #[test]
    fn tss_desc_base_split_correctly()
    {
        let addr: u64 = 0x1234_5678_9ABC_DEF0;
        let (lo, hi) = tss_desc(addr);
        // high descriptor = base[63:32]
        assert_eq!(hi & 0xFFFF_FFFF, addr >> 32);
        // base[23:0] in lo bits [39:16]
        assert_eq!((lo >> 16) & 0xFF_FFFF, addr & 0xFF_FFFF);
        // base[31:24] in lo bits [63:56]
        assert_eq!((lo >> 56) & 0xFF, (addr >> 24) & 0xFF);
    }

    #[test]
    fn selector_constants_match_gdt_layout()
    {
        // index 1 × 8 = 0x08, etc.
        assert_eq!(KERNEL_CS, 0x08);
        assert_eq!(KERNEL_DS, 0x10);
        assert_eq!(USER_DS, 0x1B); // index 3 × 8 | RPL 3
        assert_eq!(USER_CS, 0x23); // index 4 × 8 | RPL 3
        assert_eq!(TSS_SEL, 0x28);
    }
}
