// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/idt.rs

//! Interrupt Descriptor Table for x86-64.
//!
//! Provides:
//! - A 256-entry IDT in BSS.
//! - Naked ISR stubs for exception vectors 0–31 (macro-generated).
//! - Stubs for the APIC timer (vector 32) and spurious (vector 255).
//! - A common exception handler that prints diagnostics and halts.
//!
//! # Exception vector groups
//! Vectors with a hardware-pushed error code: 8, 10, 11, 12, 13, 14, 17, 21, 29, 30.
//! All others: a dummy 0 is pushed by the stub to keep the frame layout uniform.
//!
//! # IST assignments
//! - Vector 8  (Double Fault): IST1
//! - Vector 2  (NMI):          IST2
//!
//! # Modification notes
//! - To register a device IRQ: add a new stub (or reuse a range), call
//!   `set_gate` with the target vector, and implement the handler function.
//! - To change IST assignments: update the `IST` argument in the `isr_stub!`
//!   invocation and ensure the matching IST stack is configured in the TSS.

use super::gdt::KERNEL_CS;
#[cfg(not(test))]
use crate::fatal;

// ── IdtEntry ──────────────────────────────────────────────────────────────────

/// A single 128-bit (16-byte) IDT gate descriptor.
///
/// Encodes the handler offset (split into three parts), code segment selector,
/// IST index, gate type, DPL, and present bit.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct IdtEntry
{
    /// Handler offset bits [15:0].
    offset_low: u16,
    /// Code segment selector (must be a 64-bit code segment).
    selector: u16,
    /// IST index (bits [2:0]); 0 = use RSP from TSS, 1–7 = IST stack.
    ist: u8,
    /// Type and attributes: P | DPL | 0 | gate_type.
    type_attr: u8,
    /// Handler offset bits [31:16].
    offset_mid: u16,
    /// Handler offset bits [63:32].
    offset_high: u32,
    /// Reserved, must be zero.
    _reserved: u32,
}

impl IdtEntry
{
    /// Construct a present interrupt gate pointing to `handler`.
    ///
    /// - `ist`: 0 = use RSP0 from TSS, 1–7 = use IST[ist] from TSS.
    /// - `dpl`: descriptor privilege level (0 for kernel-only gates).
    /// - Type = 0xE (64-bit interrupt gate, clears IF on entry).
    pub fn new(handler: u64, ist: u8, dpl: u8) -> Self
    {
        Self {
            offset_low: (handler & 0xFFFF) as u16,
            selector: KERNEL_CS,
            ist: ist & 0x7,
            // P=1, DPL, 0, type=0xE (64-bit interrupt gate)
            type_attr: 0x80 | ((dpl & 3) << 5) | 0x0E,
            offset_mid: ((handler >> 16) & 0xFFFF) as u16,
            offset_high: (handler >> 32) as u32,
            _reserved: 0,
        }
    }
}

// ── IDT storage ───────────────────────────────────────────────────────────────

/// 256-entry IDT in BSS.
///
/// Written only during single-threaded boot init.
#[cfg(not(test))]
static mut IDT: [IdtEntry; 256] = [IdtEntry {
    offset_low: 0,
    selector: 0,
    ist: 0,
    type_attr: 0,
    offset_mid: 0,
    offset_high: 0,
    _reserved: 0,
}; 256];

// ── IDTR ──────────────────────────────────────────────────────────────────────

#[repr(C, packed)]
struct Idtr
{
    limit: u16,
    base: u64,
}

// ── ExceptionFrame ────────────────────────────────────────────────────────────

/// Register state saved on the stack by the ISR stubs.
///
/// Layout (from lowest address / first pushed):
/// ```text
/// [rsp+0]  vector        (pushed by stub)
/// [rsp+8]  error_code    (hardware or dummy 0)
/// [rsp+16] rip           (hardware)
/// [rsp+24] cs            (hardware)
/// [rsp+32] rflags        (hardware)
/// [rsp+40] rsp           (hardware; pushed on CPL change only)
/// [rsp+48] ss            (hardware; pushed on CPL change only)
/// ```
#[repr(C)]
pub struct ExceptionFrame
{
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// ── Common exception handler ──────────────────────────────────────────────────

/// Called from all exception stubs with a pointer to the exception frame.
///
/// Prints diagnostics and calls `fatal()`. Never returns.
///
/// # Safety
/// `frame` must point to a valid `ExceptionFrame` on the current stack.
#[cfg(not(test))]
unsafe extern "C" fn common_exception_handler(frame: *const ExceptionFrame) -> !
{
    // SAFETY: frame pointer is valid — constructed by ISR stubs on this stack.
    let f = unsafe { &*frame };
    crate::kprintln!(
        "EXCEPTION: vector={} error_code={:#x} rip={:#x} cs={:#x} rflags={:#x}",
        f.vector,
        f.error_code,
        f.rip,
        f.cs,
        f.rflags
    );
    fatal("unhandled exception");
}

// ── ISR stub macro ────────────────────────────────────────────────────────────

/// Generate a naked ISR stub for `$vector`.
///
/// If `$has_error_code` is `false`, the stub pushes a dummy 0 before the
/// vector so the stack frame is uniform for `common_exception_handler`.
///
/// Stack on entry to the common handler (from RSP downward):
/// ```text
/// [rsp]    vector (u64)
/// [rsp+8]  error_code (u64)   — hardware or dummy
/// [rsp+16] rip / cs / rflags  — hardware
/// ```
macro_rules! isr_stub {
    ($name:ident, $vector:expr, has_error_code = false, ist = $ist:expr) => {
        #[cfg(not(test))]
        #[unsafe(naked)]
        unsafe extern "C" fn $name()
        {
            core::arch::naked_asm!(
                "push 0",                     // dummy error code
                concat!("push ", $vector),    // vector number
                "jmp {handler}",
                handler = sym common_exception_trampoline,
            );
        }
    };
    ($name:ident, $vector:expr, has_error_code = true, ist = $ist:expr) => {
        #[cfg(not(test))]
        #[unsafe(naked)]
        unsafe extern "C" fn $name()
        {
            core::arch::naked_asm!(
                concat!("push ", $vector), // vector number (error code already on stack)
                "jmp {handler}",
                handler = sym common_exception_trampoline,
            );
        }
    };
}

/// Common trampoline: adjusts the stack and calls `common_exception_handler`.
///
/// At entry, the stack holds:
/// ```text
/// [rsp+0]  vector
/// [rsp+8]  error_code
/// [rsp+16] rip
/// [rsp+24] cs
/// [rsp+32] rflags
/// [rsp+40] rsp (if CPL change)
/// [rsp+48] ss  (if CPL change)
/// ```
/// We pass `rsp` as the first argument (pointer to ExceptionFrame).
#[cfg(not(test))]
#[unsafe(naked)]
unsafe extern "C" fn common_exception_trampoline()
{
    core::arch::naked_asm!(
        // rsp now points at the vector field — that is our ExceptionFrame.
        "mov rdi, rsp",
        "call {handler}",
        // common_exception_handler never returns; ud2 guards against it.
        "ud2",
        handler = sym common_exception_handler,
    );
}

// ── Exception stubs ───────────────────────────────────────────────────────────
// Vectors with hardware error codes: 8, 10, 11, 12, 13, 14, 17, 21, 29, 30.

isr_stub!(isr0, 0, has_error_code = false, ist = 0);
isr_stub!(isr1, 1, has_error_code = false, ist = 0);
isr_stub!(isr2, 2, has_error_code = false, ist = 2); // NMI — IST2
isr_stub!(isr3, 3, has_error_code = false, ist = 0);
isr_stub!(isr4, 4, has_error_code = false, ist = 0);
isr_stub!(isr5, 5, has_error_code = false, ist = 0);
isr_stub!(isr6, 6, has_error_code = false, ist = 0);
isr_stub!(isr7, 7, has_error_code = false, ist = 0);
isr_stub!(isr8, 8, has_error_code = true, ist = 1); // Double Fault — IST1
isr_stub!(isr9, 9, has_error_code = false, ist = 0);
isr_stub!(isr10, 10, has_error_code = true, ist = 0);
isr_stub!(isr11, 11, has_error_code = true, ist = 0);
isr_stub!(isr12, 12, has_error_code = true, ist = 0);
isr_stub!(isr13, 13, has_error_code = true, ist = 0);
isr_stub!(isr14, 14, has_error_code = true, ist = 0);
isr_stub!(isr15, 15, has_error_code = false, ist = 0);
isr_stub!(isr16, 16, has_error_code = false, ist = 0);
isr_stub!(isr17, 17, has_error_code = true, ist = 0);
isr_stub!(isr18, 18, has_error_code = false, ist = 0);
isr_stub!(isr19, 19, has_error_code = false, ist = 0);
isr_stub!(isr20, 20, has_error_code = false, ist = 0);
isr_stub!(isr21, 21, has_error_code = true, ist = 0);
isr_stub!(isr22, 22, has_error_code = false, ist = 0);
isr_stub!(isr23, 23, has_error_code = false, ist = 0);
isr_stub!(isr24, 24, has_error_code = false, ist = 0);
isr_stub!(isr25, 25, has_error_code = false, ist = 0);
isr_stub!(isr26, 26, has_error_code = false, ist = 0);
isr_stub!(isr27, 27, has_error_code = false, ist = 0);
isr_stub!(isr28, 28, has_error_code = false, ist = 0);
isr_stub!(isr29, 29, has_error_code = true, ist = 0);
isr_stub!(isr30, 30, has_error_code = true, ist = 0);
isr_stub!(isr31, 31, has_error_code = false, ist = 0);

// ── Timer and spurious stubs ──────────────────────────────────────────────────

/// APIC timer ISR stub (vector 32).
///
/// Calls `timer::timer_isr`, which increments the tick counter and sends EOI.
#[cfg(not(test))]
#[unsafe(naked)]
pub unsafe extern "C" fn isr_timer()
{
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "call {handler}",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
        handler = sym super::timer::timer_isr,
    );
}

/// Spurious interrupt handler (vector 255).
///
/// Spurious interrupts are not acknowledged via EOI — see Intel SDM §10.9.
#[cfg(not(test))]
#[unsafe(naked)]
unsafe extern "C" fn isr_spurious()
{
    core::arch::naked_asm!("iretq");
}

// ── IDT population ────────────────────────────────────────────────────────────

/// Populate the IDT and execute `lidt`.
///
/// Must be called once during boot from a single-threaded context, after
/// the GDT is loaded (since gate descriptors reference `KERNEL_CS`).
///
/// # Safety
/// Must execute at ring 0.
#[cfg(not(test))]
pub unsafe fn init()
{
    // SAFETY: single-threaded boot.
    let idt = unsafe { &mut *core::ptr::addr_of_mut!(IDT) };

    // Helper: set gate for `vec` pointing to `handler` (unsafe extern "C" fn)
    // with IST index `ist`. Casts through `*const ()` to avoid lint.
    let mut set = |vec: usize, handler: unsafe extern "C" fn(), ist: u8| {
        idt[vec] = IdtEntry::new(handler as *const () as u64, ist, 0);
    };

    // Exception gates (vectors 0–31).
    set(0, isr0, 0);
    set(1, isr1, 0);
    set(2, isr2, 2); // NMI — IST2
    set(3, isr3, 0);
    set(4, isr4, 0);
    set(5, isr5, 0);
    set(6, isr6, 0);
    set(7, isr7, 0);
    set(8, isr8, 1); // Double Fault — IST1
    set(9, isr9, 0);
    set(10, isr10, 0);
    set(11, isr11, 0);
    set(12, isr12, 0);
    set(13, isr13, 0);
    set(14, isr14, 0);
    set(15, isr15, 0);
    set(16, isr16, 0);
    set(17, isr17, 0);
    set(18, isr18, 0);
    set(19, isr19, 0);
    set(20, isr20, 0);
    set(21, isr21, 0);
    set(22, isr22, 0);
    set(23, isr23, 0);
    set(24, isr24, 0);
    set(25, isr25, 0);
    set(26, isr26, 0);
    set(27, isr27, 0);
    set(28, isr28, 0);
    set(29, isr29, 0);
    set(30, isr30, 0);
    set(31, isr31, 0);

    // APIC timer and spurious.
    set(32, isr_timer, 0);
    set(255, isr_spurious, 0);

    // Load IDTR.
    let idtr = Idtr {
        limit: (core::mem::size_of_val(idt) - 1) as u16,
        base: idt.as_ptr() as u64,
    };
    // SAFETY: idtr is live on the stack; idt is valid for the lifetime of the kernel.
    unsafe {
        core::arch::asm!(
            "lidt [{0}]",
            in(reg) &idtr,
            options(readonly, nostack, preserves_flags),
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn idt_entry_new_present_interrupt_gate()
    {
        let e = IdtEntry::new(0xDEAD_BEEF_1234_5678, 0, 0);
        // Present bit (bit 7 of type_attr).
        assert!(e.type_attr & 0x80 != 0, "P not set");
        // Gate type = 0xE in low nibble.
        assert_eq!(e.type_attr & 0x0F, 0xE, "should be interrupt gate");
        // DPL = 0.
        assert_eq!((e.type_attr >> 5) & 3, 0);
    }

    #[test]
    fn idt_entry_offset_split_correctly()
    {
        let handler: u64 = 0x1234_5678_9ABC_DEF0;
        let e = IdtEntry::new(handler, 0, 0);
        assert_eq!(e.offset_low as u64, handler & 0xFFFF);
        assert_eq!(e.offset_mid as u64, (handler >> 16) & 0xFFFF);
        assert_eq!(e.offset_high as u64, (handler >> 32) & 0xFFFF_FFFF);
    }

    #[test]
    fn idt_entry_ist_stored()
    {
        let e = IdtEntry::new(0x1000, 3, 0);
        assert_eq!(e.ist & 0x7, 3);
    }

    #[test]
    fn idt_entry_selector_is_kernel_cs()
    {
        let e = IdtEntry::new(0x1000, 0, 0);
        assert_eq!(e.selector, KERNEL_CS);
    }

    #[test]
    fn idt_entry_reserved_is_zero()
    {
        let e = IdtEntry::new(0x1000, 0, 0);
        assert_eq!(e._reserved, 0);
    }
}
