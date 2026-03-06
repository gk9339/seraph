// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/console.rs

//! Kernel early console: dual serial + framebuffer output.
//!
//! Provides `init()` and the `kprint!`/`kprintln!` macros. Output goes to both
//! the serial port and the framebuffer (when available). All state is static;
//! the kernel is single-threaded during early boot and never runs concurrent
//! code before SMP is initialized.
//!
//! Unlike the bootloader console, the kernel is a native ELF, so `core::fmt`
//! trait objects and vtable dispatch work on all architectures. The macros
//! accept full format arguments.

use crate::arch::current::console::{serial_init, serial_write_byte};
use crate::framebuffer::FramebufferWriter;
use crate::mm::paging::phys_to_virt;
use boot_protocol::BootInfo;

/// Static console state. Single-threaded early boot: no locking required.
static mut CONSOLE: Console = Console {
    serial_ready: false,
    fb: None,
};

/// Internal console state.
struct Console
{
    serial_ready: bool,
    fb: Option<FramebufferWriter>,
}

impl core::fmt::Write for Console
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result
    {
        for byte in s.bytes()
        {
            if self.serial_ready
            {
                if byte == b'\n'
                {
                    // Insert CR before LF for serial terminals.
                    // SAFETY: serial_init was called during console::init.
                    unsafe {
                        serial_write_byte(b'\r');
                    }
                }
                // SAFETY: serial_init was called during console::init.
                unsafe {
                    serial_write_byte(byte);
                }
            }

            if let Some(ref mut fb) = self.fb
            {
                // SAFETY: fb was constructed from a valid FramebufferInfo.
                unsafe {
                    fb.write_byte(byte);
                }
            }
        }
        Ok(())
    }
}

/// Initialize the kernel early console.
///
/// Initializes the serial backend and, if a framebuffer is present in
/// `boot_info`, the framebuffer backend. Must be called once during Phase 1
/// before any `kprint!` usage.
///
/// # Safety
/// Must be called at most once, from the single kernel boot thread, with a
/// valid `boot_info` pointer (Phase 0 validation must have passed).
pub unsafe fn init(boot_info: &BootInfo)
{
    // SAFETY: serial_init is called exactly once at kernel entry.
    unsafe {
        serial_init();
    }
    // SAFETY: CONSOLE is only accessed from the single boot thread.
    unsafe {
        (*core::ptr::addr_of_mut!(CONSOLE)).serial_ready = true;
    }

    // Initialize framebuffer if present.
    // SAFETY: boot_info.framebuffer describes a valid, accessible region (or
    // physical_base == 0, which FramebufferWriter::new handles gracefully).
    let writer = unsafe { FramebufferWriter::new(&boot_info.framebuffer) };
    // SAFETY: CONSOLE is only accessed from the single boot thread.
    unsafe {
        (*core::ptr::addr_of_mut!(CONSOLE)).fb = writer;
    }
}

/// Repoint the framebuffer to its direct-map virtual address.
///
/// Called after Phase 3 activates the kernel's page tables. Converts
/// `fb_phys` to `DIRECT_MAP_BASE + fb_phys` and calls `FramebufferWriter::rebase`
/// so subsequent output writes to the correct virtual address.
///
/// Does nothing if no framebuffer is present (`fb_phys == 0`).
///
/// # Safety
/// Must be called only after `init_kernel_page_tables` has returned
/// successfully and the direct physical map is active.
pub unsafe fn rebase_framebuffer(fb_phys: u64)
{
    if fb_phys == 0
    {
        return;
    }
    // SAFETY: CONSOLE is only accessed from the single boot thread.
    let console = unsafe { &mut *core::ptr::addr_of_mut!(CONSOLE) };
    if let Some(ref mut fb) = console.fb
    {
        let new_base = phys_to_virt(fb_phys) as *mut u8;
        // SAFETY: new_base is the direct-map VA of the framebuffer physical memory,
        // which is now mapped R/W by the kernel's page tables.
        unsafe {
            fb.rebase(new_base);
        }
    }
}

/// Write a formatted string to the kernel console.
///
/// Forwards to `core::fmt::Write` on the static `CONSOLE`. Must be called
/// only after `console::init()`.
///
/// # Safety
/// `console::init` must have been called before this function.
pub unsafe fn console_write_fmt(args: core::fmt::Arguments)
{
    use core::fmt::Write;
    // SAFETY: CONSOLE is only accessed from the single boot thread.
    // SAFETY: raw pointer avoids the static_mut_refs lint; single-threaded boot.
    let console = unsafe { &mut *core::ptr::addr_of_mut!(CONSOLE) };
    // Ignore fmt errors: we have no fallback output channel.
    let _ = console.write_fmt(args);
}

/// Print a formatted string to the kernel console.
///
/// Accepts the same format arguments as `std::print!`. Requires
/// `console::init()` to have been called.
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        // SAFETY: console is initialized before any macro usage.
        unsafe {
            $crate::console::console_write_fmt(format_args!($($arg)*));
        }
    };
}

/// Print a formatted string followed by `\n` to the kernel console.
///
/// Accepts the same format arguments as `std::println!`. Requires
/// `console::init()` to have been called.
#[macro_export]
macro_rules! kprintln {
    () => {
        $crate::kprint!("\n")
    };
    ($($arg:tt)*) => {
        $crate::kprint!("{}\n", format_args!($($arg)*))
    };
}
