// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/main.rs

//! ktest — Seraph kernel test binary.
//!
//! Loaded by the kernel in place of real init (set `init=ktest` in boot.conf).
//! Receives the same initial capability set that init would, then:
//!
//! 1. **Tier 1** (`unit/`)        — exercises every kernel syscall in isolation.
//! 2. **Tier 2** (`integration/`) — cross-subsystem scenario tests.
//! 3. **Tier 3** (`bench/`)       — placeholder for future timing/profiling.
//!
//! Results are printed to the kernel serial console via `SYS_DEBUG_LOG`.
//! Each test prints `PASS` or `FAIL`. A summary follows. ktest then exits.
//!
//! See `ktest/README.md` for the full test structure and output format.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

mod bench;
mod frame_pool;
mod integration;
mod unit;

// ── Test infrastructure ───────────────────────────────────────────────────────

/// Return type for every test function.
///
/// On failure the `Err` string is a static reason message logged immediately
/// after the test name.
pub type TestResult = Result<(), &'static str>;

/// Counts tests that returned `Ok(())`.
pub static PASS_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Counts tests that returned `Err(...)`.
pub static FAIL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Run a named unit test and update the global pass/fail counters.
///
/// `$name` must be a string literal (used with `concat!` for zero-cost
/// log messages). `$body` is an expression evaluating to [`TestResult`].
///
/// Prints one line per test: `ktest: PASS <name>` or `ktest: FAIL <name>`
/// followed by the failure reason. Tests always continue past failures.
///
/// To add a new unit test: write a function returning `TestResult`, then
/// call `run_test!("subsystem::test_name", fn_name(ctx))` in the relevant
/// `run_all` function.
#[macro_export]
macro_rules! run_test {
    ($name:literal, $body:expr) => {{
        let result: $crate::TestResult = { $body };
        match result
        {
            Ok(()) =>
            {
                $crate::klog(concat!("ktest: PASS ", $name));
                $crate::PASS_COUNT.fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
            }
            Err(reason) =>
            {
                $crate::klog(concat!("ktest: FAIL ", $name));
                $crate::klog(reason);
                $crate::FAIL_COUNT.fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
            }
        }
    }};
}

/// Run a named integration test and update the global pass/fail counters.
///
/// Like [`run_test!`] but emits a `"ktest: <name> starting"` line before
/// running the test body. Integration tests typically emit step-by-step
/// progress logs from within their body; the starting line marks their
/// beginning and the PASS/FAIL line marks their end.
///
/// To add a new integration test: implement it in `integration/`, declare
/// it with `pub mod`, then call `run_integration_test!` in `run_all`.
#[macro_export]
macro_rules! run_integration_test {
    ($name:literal, $body:expr) => {{
        $crate::klog(concat!("ktest: ", $name, " starting"));
        let result: $crate::TestResult = { $body };
        match result
        {
            Ok(()) =>
            {
                $crate::klog(concat!("ktest: PASS ", $name));
                $crate::PASS_COUNT.fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
            }
            Err(reason) =>
            {
                $crate::klog(concat!("ktest: FAIL ", $name));
                $crate::klog(reason);
                $crate::FAIL_COUNT.fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
            }
        }
    }};
}

/// Context passed to all test functions.
///
/// Carries the two resources that many tests need: the ktest `AddressSpace` cap
/// and the IPC buffer pointer. Pass by shared reference to test functions.
pub struct TestContext
{
    /// ktest's own `AddressSpace` capability slot, provided by the kernel.
    ///
    /// Used for memory management tests, thread creation (threads must be
    /// bound to an address space), and hardware access tests.
    pub aspace_cap: u32,

    /// Pointer to the registered IPC buffer page (4 KiB, page-aligned).
    ///
    /// Registered with `ipc_buffer_set` before any tests run. Pass to
    /// `read_recv_caps` to inspect received capability indices after an
    /// `ipc_recv` or `ipc_call` returns.
    pub ipc_buf: *const u64,
}

/// 16 KiB stack for a child thread, aligned per the System V ABI.
///
/// Declare one as `static mut CHILD_STACK: ChildStack = ChildStack::ZERO` in
/// any test module that spawns a child thread. Pass `ChildStack::top(ptr)` to
/// `thread_configure` as the initial stack pointer.
///
/// Each test file that needs a child thread declares its own static stack so
/// stacks never alias across concurrent (or sequential) test boundaries.
#[allow(dead_code)] // Field is CPU stack memory; only the hardware stack pointer accesses it, not Rust code.
#[repr(align(16))]
pub struct ChildStack([u8; 16384]);

impl ChildStack
{
    pub const ZERO: ChildStack = ChildStack([0u8; 16384]);

    /// Virtual address of the top of the stack at `ptr` (one past the last byte).
    ///
    /// Thread stacks grow downward; this is the value to pass as the initial
    /// stack pointer. Takes a raw pointer to avoid creating a reference to a
    /// `static mut` — pass `core::ptr::addr_of!(STACK)` as the argument.
    #[must_use]
    pub fn top(ptr: *const Self) -> u64
    {
        ptr as u64 + 16384
    }
}

// ── IPC buffer ────────────────────────────────────────────────────────────────

/// Static IPC buffer — 4 KiB, page-aligned.
///
/// Registered once in `run()` via `ipc_buffer_set`. The kernel writes received
/// message data and capability slot indices here. Tests read it via
/// `read_recv_caps(ctx.ipc_buf)`.
#[repr(C, align(4096))]
struct IpcBuf([u64; 512]);

// SAFETY: ktest is single-threaded on the main test path. Child threads do not
// call ipc_recv, so the kernel never writes the IPC buffer from child context.
static mut IPC_BUF: IpcBuf = IpcBuf([0u64; 512]);

// ── Entry point ───────────────────────────────────────────────────────────────

/// Kernel entry point for ktest.
///
/// `aspace_cap` — slot index in ktest's `CSpace` pointing to its own `AddressSpace`.
/// Provided by the kernel as the initial argument (same as for real init).
#[no_mangle]
pub extern "C" fn _start(aspace_cap: u32) -> !
{
    run(aspace_cap)
}

fn run(aspace_cap: u32) -> !
{
    klog("ktest: starting");

    // Register the IPC buffer before any IPC syscall. All Tier 1 IPC tests
    // and integration tests that use ipc_recv or read_recv_caps depend on this.
    // SAFETY: IPC_BUF is a page-aligned static in ktest's BSS; single-threaded here.
    let ipc_buf_ptr = core::ptr::addr_of!(IPC_BUF).cast::<u64>();
    syscall::ipc_buffer_set(ipc_buf_ptr as u64).unwrap_or_else(|_| {
        klog("ktest: FATAL: ipc_buffer_set failed");
        halt()
    });

    // Initialize the frame pool before running tests. Tests consume frame caps
    // via splits; without pooling, resources are exhausted after ~10 tests.
    // SAFETY: Called once before any tests run; no concurrent access yet.
    unsafe {
        frame_pool::init(aspace_cap);
    }

    let ctx = TestContext {
        aspace_cap,
        ipc_buf: ipc_buf_ptr,
    };

    // ── Tier 1: per-syscall isolation ─────────────────────────────────────────
    klog("ktest: --- Tier 1: syscall isolation ---");
    unit::run_all(&ctx);

    // ── Tier 2: cross-subsystem integration ───────────────────────────────────
    klog("ktest: --- Tier 2: integration ---");
    integration::run_all(&ctx);

    // ── Tier 3: benchmarks ────────────────────────────────────────────────────
    klog("ktest: --- Tier 3: benchmarks ---");
    bench::run_all(&ctx);

    // ── Summary ───────────────────────────────────────────────────────────────
    let passed = PASS_COUNT.load(Ordering::Relaxed);
    let failed = FAIL_COUNT.load(Ordering::Relaxed);
    log_u64("ktest: passed=", passed as u64);
    log_u64("ktest: failed=", failed as u64);
    if failed == 0
    {
        klog("ktest: ALL TESTS PASSED");
    }
    else
    {
        klog("ktest: SOME TESTS FAILED");
    }

    syscall::thread_exit()
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Write a string to the kernel serial console via `SYS_DEBUG_LOG`.
///
/// Errors from `debug_log` are silently ignored — if logging itself fails,
/// there is nothing useful to do.
#[inline]
pub fn klog(msg: &str)
{
    syscall::debug_log(msg).ok();
}

/// Log a decimal `u64` value prefixed by a static string.
///
/// Uses a fixed 24-byte stack buffer — no heap required.
pub fn log_u64(prefix: &str, value: u64)
{
    let mut buf = [0u8; 24];
    let mut n = value;
    let mut len = 0usize;
    if n == 0
    {
        buf[0] = b'0';
        len = 1;
    }
    else
    {
        while n > 0
        {
            buf[len] = b'0' + (n % 10) as u8;
            n /= 10;
            len += 1;
        }
        buf[..len].reverse();
    }

    let num_str = core::str::from_utf8(&buf[..len]).unwrap_or("?");
    let mut msg = [0u8; 128];
    let pb = prefix.as_bytes();
    let plen = pb.len().min(msg.len());
    msg[..plen].copy_from_slice(&pb[..plen]);
    let nlen = num_str.len().min(msg.len() - plen);
    msg[plen..plen + nlen].copy_from_slice(&num_str.as_bytes()[..nlen]);
    let total = plen + nlen;
    if let Ok(s) = core::str::from_utf8(&msg[..total])
    {
        klog(s);
    }
}

/// Log a kernel version packed as a `u64` in `MAJOR.MINOR.PATCH` encoding.
///
/// Prints `prefix` followed by `"vMAJOR.MINOR.PATCH"` (e.g. `"v0.0.1"`).
/// Uses the same shift/mask encoding as `syscall_abi::KERNEL_VERSION`:
/// `major = ver >> 32`, `minor = (ver >> 16) & 0xFFFF`, `patch = ver & 0xFFFF`.
pub fn log_version(prefix: &str, ver: u64)
{
    // Write a u64 as decimal into buf starting at pos; returns new pos.
    fn write_num(buf: &mut [u8], pos: usize, mut n: u64) -> usize
    {
        if n == 0
        {
            if pos < buf.len()
            {
                buf[pos] = b'0';
                return pos + 1;
            }
            return pos;
        }
        let start = pos;
        let mut end = pos;
        while n > 0 && end < buf.len()
        {
            buf[end] = b'0' + (n % 10) as u8;
            n /= 10;
            end += 1;
        }
        buf[start..end].reverse();
        end
    }

    let major = ver >> 32;
    let minor = (ver >> 16) & 0xFFFF;
    let patch = ver & 0xFFFF;
    let mut buf = [0u8; 128];
    let pb = prefix.as_bytes();
    let plen = pb.len().min(buf.len());
    buf[..plen].copy_from_slice(&pb[..plen]);
    let mut pos = plen;

    if pos < buf.len()
    {
        buf[pos] = b'v';
        pos += 1;
    }
    pos = write_num(&mut buf, pos, major);
    if pos < buf.len()
    {
        buf[pos] = b'.';
        pos += 1;
    }
    pos = write_num(&mut buf, pos, minor);
    if pos < buf.len()
    {
        buf[pos] = b'.';
        pos += 1;
    }
    pos = write_num(&mut buf, pos, patch);

    if let Ok(s) = core::str::from_utf8(&buf[..pos])
    {
        klog(s);
    }
}

/// Spin forever. Used for fatal errors where ktest cannot continue.
pub fn halt() -> !
{
    loop
    {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    klog("ktest: panic");
    halt()
}
