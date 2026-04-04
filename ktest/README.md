# ktest — Seraph kernel test binary

ktest is a `no_std` binary that runs as the kernel's "init" process for the
purpose of end-to-end kernel testing. It receives the same initial capability
set that real init would, exercises every kernel syscall, and reports results
to the serial console before exiting.

## Activating ktest

Edit `rootfs/EFI/seraph/boot.conf` and change the `init` field:

```
init=ktest
```

Then rebuild (`cargo xtask build`) and run (`cargo xtask run`). Restore
`init=init` when finished.

## Test structure

Tests are organised across three tiers. Each tier lives in its own source
directory.

### Tier 1 — `src/unit/`

Per-syscall isolation tests. Every kernel syscall has at least one positive-path
test and the most important negative paths (wrong rights, invalid arguments,
wrong object state). Files are grouped by kernel subsystem, mirroring the
kernel's own source layout.

| File | Syscalls |
|---|---|
| `cap.rs` | `SYS_CAP_CREATE_*`, `CAP_COPY`, `CAP_MOVE`, `CAP_INSERT`, `CAP_DERIVE`, `CAP_REVOKE`, `CAP_DELETE` |
| `mm.rs` | `SYS_MEM_MAP/UNMAP/PROTECT`, `SYS_FRAME_SPLIT`, `SYS_ASPACE_QUERY` |
| `signal.rs` | `SYS_SIGNAL_SEND`, `SYS_SIGNAL_WAIT` |
| `event.rs` | `SYS_EVENT_POST`, `SYS_EVENT_RECV` |
| `wait_set.rs` | `SYS_WAIT_SET_ADD/REMOVE/WAIT` |
| `ipc.rs` | `SYS_IPC_CALL`, `SYS_IPC_REPLY`, `SYS_IPC_RECV`, `SYS_IPC_BUFFER_SET` |
| `thread.rs` | `SYS_THREAD_START/STOP/YIELD/EXIT/CONFIGURE/SET_PRIORITY/SET_AFFINITY/READ_REGS/WRITE_REGS` |
| `hw.rs` | `SYS_MMIO_MAP`, `SYS_DMA_GRANT`, `SYS_IRQ_REGISTER/ACK`, `SYS_IOPORT_BIND` |
| `sysinfo.rs` | `SYS_SYSTEM_INFO`, `SYS_DEBUG_LOG` |

Adding a new syscall means adding a section in the appropriate file here.

### Tier 2 — `src/integration/`

Cross-subsystem scenario tests that exercise realistic multi-syscall workflows.
These catch bugs that unit tests miss — e.g. capability rights surviving an IPC
transfer, thread state after stop+write_regs+resume, wait set ordering under
concurrent signal and queue events.

| File | Scenario |
|---|---|
| `thread_lifecycle.rs` | Full thread lifecycle: create → configure → start → stop → read\_regs → write\_regs → resume → exit |
| `cap_transfer.rs` | Cap rights flow through an IPC endpoint round-trip |
| `wait_concurrency.rs` | Wait set with concurrent signal + queue sources |
| `memory_lifecycle.rs` | Frame split → map → protect → unmap with aspace\_query at each step |

### Tier 3 — `src/bench/` (placeholder)

Reserved for future timing and profiling infrastructure. Requires kernel-side
timing support (e.g. a clock syscall or rdtsc/cycle-CSR exposure) before
meaningful numbers can be collected. The module exists as a placeholder so the
structure is clear when that work begins.

## Test infrastructure

Defined in `src/main.rs`:

- `TestResult` — `Result<(), &'static str>` — no heap, no allocation.
- `run_test!(name, body)` — macro that logs the test name, runs `body`,
  records PASS or FAIL (with reason), and never panics.
- `TestContext` — thin struct carrying `aspace_cap` and the IPC buffer pointer,
  passed by reference to every test function.
- `PASS_COUNT` / `FAIL_COUNT` — atomic counters updated by `run_test!`.
- `klog(msg)` / `log_u64(prefix, value)` — heap-free logging utilities.

## Output format

Each test produces two lines on the serial console:

```
ktest: run  cap::create_signal
ktest: PASS cap::create_signal
```

or on failure:

```
ktest: run  cap::create_signal
ktest: FAIL cap::create_signal
ktest: <reason string>
```

A summary is printed at the end:

```
ktest: passed=42
ktest: failed=0
ktest: ALL TESTS PASSED
```
