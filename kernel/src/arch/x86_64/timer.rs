// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/timer.rs

//! x86-64 preemption timer using the local APIC timer (xAPIC).
//!
//! # Calibration
//! The APIC timer frequency is CPU-specific and unknown at compile time.
//! We calibrate it against the 8254 PIT (Programmable Interval Timer) by:
//!
//! 1. Programming PIT channel 2 for a ~10 ms one-shot countdown.
//! 2. Starting the APIC timer with initial count `0xFFFF_FFFF`, divide-by-16.
//! 3. Spinning until the PIT expires.
//! 4. Reading the remaining APIC count to derive ticks per 10 ms.
//!
//! The PIT output is readable on port 0x61 (bit 5: channel 2 output).
//!
//! # Timer ISR
//! The ISR increments `TICK_COUNT` and sends EOI. It is called from the naked
//! stub `idt::isr_timer`.
//!
//! # Modification notes
//! - To change the tick period: pass a different `period_us` to `init()`.
//! - To get higher resolution: reduce divide ratio and recalculate.
//! - To use TSC deadline mode: replace periodic mode programming; requires
//!   CPUID feature check and x2APIC.

use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(not(test))]
use super::interrupts;
#[cfg(not(test))]
use crate::mm::paging::DIRECT_MAP_BASE;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Vector number assigned to the APIC timer interrupt.
pub const TIMER_VECTOR: u8 = 32;

/// APIC register offsets (from `DIRECT_MAP_BASE + APIC_BASE_PHYS`).
const APIC_BASE_PHYS: u64 = 0xFEE0_0000;
const APIC_LVT_TIMER: usize = 0x320;
const APIC_TIMER_INITIAL: usize = 0x380;
const APIC_TIMER_CURRENT: usize = 0x390;
const APIC_TIMER_DIVIDE: usize = 0x3E0;

/// LVT timer mode: periodic (bit 17 set).
const LVT_TIMER_PERIODIC: u32 = 1 << 17;

/// Divide-by-16 configuration for the APIC timer divide register.
const DIVIDE_BY_16: u32 = 0x3;

// PIT I/O port numbers.
const PIT_CMD: u16 = 0x43;
const PIT_CH2: u16 = 0x42;
const PIT_GATE: u16 = 0x61;

// PIT oscillator frequency (Hz).
const PIT_HZ: u64 = 1_193_182;

// ── Tick state ────────────────────────────────────────────────────────────────

/// Monotonically increasing tick counter; incremented by the timer ISR.
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// APIC ticks per second (computed during calibration).
static TICKS_PER_SEC: AtomicU64 = AtomicU64::new(0);

// ── APIC register helpers ─────────────────────────────────────────────────────

#[cfg(not(test))]
unsafe fn apic_write(offset: usize, val: u32)
{
    let vaddr = (DIRECT_MAP_BASE + APIC_BASE_PHYS) as usize + offset;
    unsafe {
        core::ptr::write_volatile(vaddr as *mut u32, val);
    }
}

#[cfg(not(test))]
fn apic_read(offset: usize) -> u32
{
    let vaddr = (DIRECT_MAP_BASE + APIC_BASE_PHYS) as usize + offset;
    unsafe { core::ptr::read_volatile(vaddr as *const u32) }
}

// ── Port I/O helpers ──────────────────────────────────────────────────────────

/// Write `val` to x86 I/O port `port`.
#[cfg(not(test))]
unsafe fn outb(port: u16, val: u8)
{
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nostack, nomem),
        );
    }
}

/// Read a byte from x86 I/O port `port`.
#[cfg(not(test))]
unsafe fn inb(port: u16) -> u8
{
    let val: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port,
            out("al") val,
            options(nostack, nomem),
        );
    }
    val
}

// ── Calibration ───────────────────────────────────────────────────────────────

/// Calibrate the APIC timer using the 8254 PIT as the reference.
///
/// Programs PIT channel 2 for `pit_ticks` counts (~= `pit_ms` ms), starts
/// the APIC timer at full count with divide-by-16, spins until PIT expires,
/// then returns the number of APIC ticks that elapsed.
///
/// Returns `(apic_ticks_per_pit_interval, pit_ms)`.
#[cfg(not(test))]
unsafe fn calibrate_apic_timer() -> u64
{
    // PIT calibration interval: ~10 ms.
    const PIT_CALIBRATION_MS: u64 = 10;
    // PIT counts for 10 ms.
    let pit_counts = (PIT_HZ * PIT_CALIBRATION_MS / 1000) as u16;

    // Gate channel 2: clear bit 0 (disable gate), keep bit 1.
    // SAFETY: port I/O at ring 0.
    unsafe {
        let gate = inb(PIT_GATE);
        outb(PIT_GATE, (gate & 0xFD) | 0x00); // gate off
    }

    // Program PIT channel 2: mode 0 (interrupt on terminal count), binary.
    // Command byte: channel=10 (ch2), access=11 (lo+hi byte), mode=000, BCD=0.
    unsafe {
        outb(PIT_CMD, 0b10110000);
        outb(PIT_CH2, (pit_counts & 0xFF) as u8);
        outb(PIT_CH2, (pit_counts >> 8) as u8);
    }

    // Set APIC timer divide-by-16 and start with max initial count.
    unsafe {
        apic_write(APIC_TIMER_DIVIDE, DIVIDE_BY_16);
        apic_write(APIC_TIMER_INITIAL, 0xFFFF_FFFF);
    }

    // Enable PIT channel 2 gate (bit 0 of port 0x61).
    unsafe {
        let gate = inb(PIT_GATE);
        outb(PIT_GATE, gate | 0x01);
    }

    // Spin until PIT output (bit 5 of port 0x61) goes high.
    unsafe {
        while inb(PIT_GATE) & 0x20 == 0
        {
            core::hint::spin_loop();
        }
    }

    // Stop the APIC timer.
    let remaining = apic_read(APIC_TIMER_CURRENT);
    unsafe {
        apic_write(APIC_TIMER_INITIAL, 0);
    }

    let elapsed = 0xFFFF_FFFFu64 - remaining as u64;
    // Scale to ticks/second: elapsed ticks in PIT_CALIBRATION_MS ms.
    elapsed * 1000 / PIT_CALIBRATION_MS
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Initialise the APIC timer for periodic preemption at `period_us` microseconds.
///
/// Calibrates the APIC timer frequency against the PIT, configures the timer
/// for periodic mode at vector `TIMER_VECTOR`, and **enables interrupts** (`sti`).
///
/// Must be called after `interrupts::init()`.
///
/// # Safety
/// Must execute at ring 0 from a single-threaded context.
#[cfg(not(test))]
pub unsafe fn init(period_us: u64)
{
    // Calibrate: measure APIC ticks per second.
    // SAFETY: ring-0, single-threaded.
    let tps = unsafe { calibrate_apic_timer() };
    TICKS_PER_SEC.store(tps, Ordering::Relaxed);

    // Compute initial count for the requested period.
    // Formula: initial_count = tps * period_us / 1_000_000 / divide_ratio.
    // divide_ratio = 16 (DIVIDE_BY_16).
    let initial_count = (tps * period_us / 1_000_000).max(1);

    // Configure APIC timer: periodic mode, vector TIMER_VECTOR.
    unsafe {
        apic_write(APIC_TIMER_DIVIDE, DIVIDE_BY_16);
        apic_write(APIC_LVT_TIMER, LVT_TIMER_PERIODIC | TIMER_VECTOR as u32);
        apic_write(APIC_TIMER_INITIAL, initial_count as u32);
    }

    // Enable interrupts — the timer will now fire.
    // SAFETY: IDT is loaded; timer ISR is registered.
    unsafe {
        interrupts::enable();
    }
}

/// No-op test stub: timer hardware cannot be accessed in host unit tests.
#[cfg(test)]
pub unsafe fn init(_period_us: u64) {}

/// Timer ISR body — called from the naked stub in `idt.rs`.
///
/// Increments the tick counter and sends EOI to the local APIC.
/// Must not allocate or block.
#[cfg(not(test))]
pub extern "C" fn timer_isr()
{
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
    interrupts::acknowledge(TIMER_VECTOR as u32);
}

/// Return the current monotonic tick count.
pub fn current_tick() -> u64
{
    TICK_COUNT.load(Ordering::Relaxed)
}

/// Return the number of APIC ticks per second (calibrated at boot).
pub fn ticks_per_second() -> u64
{
    TICKS_PER_SEC.load(Ordering::Relaxed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn timer_vector_is_32()
    {
        assert_eq!(TIMER_VECTOR, 32);
    }

    #[test]
    fn apic_timer_divide_offset()
    {
        assert_eq!(APIC_TIMER_DIVIDE, 0x3E0);
    }

    #[test]
    fn apic_timer_initial_offset()
    {
        assert_eq!(APIC_TIMER_INITIAL, 0x380);
    }

    #[test]
    fn tick_rate_math_10ms()
    {
        // At 100 MHz APIC clock, 10 ms = 1_000_000 ticks.
        let tps: u64 = 100_000_000;
        let period_us: u64 = 10_000; // 10 ms
        let initial_count = (tps * period_us / 1_000_000).max(1);
        assert_eq!(initial_count, 1_000_000);
    }

    #[test]
    fn tick_rate_math_1ms()
    {
        // At 100 MHz APIC clock, 1 ms = 100_000 ticks.
        let tps: u64 = 100_000_000;
        let period_us: u64 = 1_000;
        let initial_count = (tps * period_us / 1_000_000).max(1);
        assert_eq!(initial_count, 100_000);
    }

    #[test]
    fn divide_by_16_constant()
    {
        assert_eq!(DIVIDE_BY_16, 0x3);
    }

    #[test]
    fn lvt_timer_periodic_bit()
    {
        assert_eq!(LVT_TIMER_PERIODIC, 1 << 17);
    }
}
