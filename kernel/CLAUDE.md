# Kernel — AI Context

The Seraph microkernel. Handles IPC, scheduling, memory management, and capabilities.
Nothing else. See @README.md for source layout, module responsibilities, and build
structure. See root `.claude/CLAUDE.md` for project-wide invariants and coding standards.

## Environment

- `no_std` crate; no standard library
- Custom targets: `x86_64-seraph-none`, `riscv64gc-seraph-none`
- SSE/AVX/FPU are **disabled in the target spec** and must not be used before explicit
  arch init (Phase 5). Floating-point in the kernel is forbidden.
- The kernel heap is available **only after Phase 4** (slab allocator init). Before
  Phase 4, only static data and the boot stack exist — no `Box`, `Vec`, or any
  `alloc` type.

## Key Design Documents

Read the relevant document before modifying any subsystem:

- @docs/initialization.md — 11-phase boot sequence (Phase 0–10) with per-phase
  constraints and the fatal-boot-failure protocol
- @docs/syscalls.md — 43 syscalls, full ABI, register conventions, error codes
- @docs/memory-internals.md — buddy/slab/size-class allocators, TLB shootdown,
  address space and page table structures
- @docs/capability-internals.md — CSpace two-level array, derivation tree,
  revocation algorithm
- @docs/ipc-internals.md — endpoint state machine, signal fast path, event queue
  ring buffer, wait set, **lock ordering**
- @docs/scheduler.md — 32 priority levels, per-CPU run queues, SMT awareness,
  context switch, load balancing
- @docs/arch-interface.md — the 7 arch traits every implementation must satisfy:
  `Paging`, `Context`, `Interrupts`, `Timer`, `Syscall`, `Cpu`, `EarlyConsole`

## Architecture Isolation

All arch-specific code lives in `src/arch/{x86_64,riscv64}/`. Shared kernel code
accesses it only through `arch::current::*`, which re-exports the active arch module.
The `#[cfg(target_arch)]` guard exists **only** in `arch/mod.rs`.

If shared code needs a new arch-dependent operation, add it to the appropriate trait in
`docs/arch-interface.md` and implement it in both arch directories. Do not add cfg
guards anywhere else.

## Lock Ordering (Critical)

Violating the global lock ordering causes deadlock. Always acquire in this order;
never reverse. Source: `docs/ipc-internals.md` §"Lock Ordering".

```
1. Per-CPU scheduler lock
   — when acquiring two scheduler locks, always take the lower CPU ID first

2. IPC object lock (endpoint / signal / event queue)
   — never hold two IPC object locks simultaneously

3. Wait set lock
   — always acquired after the source IPC object lock that triggered notification

4. Buddy allocator lock

5. Derivation tree lock (reader or writer)
```

**Key consequence for revocation:** `SYS_CAP_REVOKE` cannot hold the derivation tree
write lock while acquiring IPC object locks. It uses a two-phase deferred cleanup:
collect affected objects under the tree lock, then clean up under individual IPC locks.
See `docs/ipc-internals.md` §"Deferred cleanup during revocation".

## Initialization Order

Phases are strictly sequential. See `docs/initialization.md` for the full 11-phase
sequence. Critical phase boundaries:

| Phase | What becomes available | Failure behaviour |
|---|---|---|
| 0 | BootInfo validated | Halt (cannot continue) |
| 2 | Buddy allocator | Halt: no usable RAM |
| 3 | Physical direct map at PHYSMAP_BASE | Halt: OOM |
| 4 | Slab allocator + kernel heap | Halt: cannot init heap |
| 5 | Interrupts, arch hardware | Halt: arch init failed |
| 7 | Capability system | Halt: cap system failed |
| 8 | Scheduler, idle threads | Halt: scheduler failed |
| 9 | Init process | Halt: no init binary |
| 10 | SMP bringup, scheduler handoff | — |

Do not use `Box`/`Vec`/any heap type before Phase 4. Do not enable interrupts before
Phase 5. Do not create capabilities before Phase 7.

## Scheduler Rules

- 32 priority levels: 0 = idle (reserved), 31 = reserved
- Normal userspace range: 1–20, default = 10
- Elevated range: 21–30 (requires Thread Control + SchedControl capability)
- Highest-priority thread selected in O(1) via `non_empty` bitmask
- Preemption occurs only at user-mode return, not mid-kernel
- No priority inheritance (by design; spinlocks do not cause priority inversion here)

## Memory Rules

- Physical direct map: `PHYSMAP_BASE = 0xFFFF800000000000` (both architectures)
- `phys_to_virt(p)` = `PHYSMAP_BASE + p`; `virt_to_phys(v)` = `v - PHYSMAP_BASE`
- Buddy allocator: power-of-two blocks, O(log n) alloc/dealloc, zone support, fallible
- Slab allocator: fixed-size caches for `TCB`, `CapabilitySlot`, `Endpoint`, etc., O(1)
- W^X enforced at page table level; never map a region as both writable and executable
- x86-64: SMEP and SMAP enabled. RISC-V: SUM bit controls user-memory access from S-mode.

## IPC Rules

- Synchronous call blocks the sender until the server replies
- Reply capability is single-use, non-storable, non-delegatable; stored in a per-thread
  kernel slot, consumed on the first use
- Small messages (≤ `MSG_DATA_WORDS_MAX` words, ≤ 4 capability slots) are
  register-only — the fast path involves no memory allocation
- Extended payloads use the per-thread IPC buffer page; still no heap allocation
- Signals: atomic OR bitmask, coalescing (multiple signals merge), lock-free fast path
- Event queues: ordered ring buffer, no coalescing, `QueueFull` error on overflow
- Capabilities in IPC are **moved** (sender loses them), not copied

## Capability Rules

- CSpace: two-level array (L1 directory + L2 pages), O(1) lookup
- Slot 0 is permanently null and cannot be used
- Rights are a bitmask; derivation can only remove rights, never add them
- **Transfer** moves a capability (changes owner, keeps derivation tree position)
- **Derivation** creates a child with equal or fewer rights (parent retains its cap)
- Revocation invalidates a capability and recursively invalidates its entire subtree
- Initial capabilities are created from nothing only during Phase 7 (boot only)

## Common Pitfalls

- Do not allocate memory in interrupt handlers
- Do not hold locks across context switches or IPC calls
- Do not use `alloc` types (`Box`, `Vec`, `Arc`) before Phase 4
- Do not add `#[cfg(target_arch)]` outside `src/arch/`
- Do not use `unwrap()`/`expect()`/`panic!()` in any production code path
- The kernel does **not** parse ACPI or Device Tree — the bootloader does that
- `IoPortRange` is x86-64 only; RISC-V has no port I/O concept; do not reference it
  in architecture-neutral code
