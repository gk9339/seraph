# Kernel Initialization Sequence

## Overview

This document describes what happens between `kernel_entry()` and the first userspace
instruction of the init process. The sequence is divided into numbered phases. Each
phase has a clear completion criterion and a defined failure mode.

A failure at any phase is fatal. The kernel does not attempt recovery from init
failures; it halts and, where possible, emits a diagnostic message. This is correct
behaviour — a kernel that cannot complete initialisation has no safe state to recover
to.

For the boot protocol contract (CPU state, BootInfo layout) that Phase 0 depends on,
see [docs/boot-protocol.md](../../docs/boot-protocol.md).

---

## Phase 0: Entry Validation

**Entry point:** `kernel_entry(boot_info: *const BootInfo)`

**What happens:**

The first act of the kernel is to validate the `BootInfo` pointer and the protocol
version field. The pointer arrives in `rdi` (x86-64) or `a0` (RISC-V) per the boot
protocol.

```
1. Verify boot_info pointer is non-null and naturally aligned for BootInfo
2. Read boot_info.version
3. Compare against BOOT_PROTOCOL_VERSION
4. If mismatch: halt immediately (cannot trust any other BootInfo fields)
5. Validate memory_map.count > 0 and memory_map.entries is non-null
6. Validate modules.count > 0 (init binary must be present)
7. Validate modules.entries[0].size > 0 (init binary has content)
```

No output is produced before step 1 succeeds — the console is not yet available.
If version validation fails, the kernel halts silently; there is no safe way to
report the error.

**Failure mode:** Infinite halt (`loop {}` / `wfi` loop). On x86-64, the halt
instruction is used in a loop to handle spurious wakeups.

**Completion criterion:** `boot_info` pointer is valid and `version` matches.

---

## Phase 1: Early Console

**What happens:**

The architecture's `EarlyConsole` implementation is initialised using the framebuffer
and/or serial port information in `BootInfo`. From this point forward, the kernel can
emit diagnostic messages.

```
1. Call arch::current::EarlyConsole::init(&boot_info)
   - x86-64: checks boot_info.framebuffer.physical_base; if non-zero,
     initialises a simple pixel-writing framebuffer console;
     also attempts to initialise a COM1 serial port at 115200 8N1
   - RISC-V: uses SBI console (sbi_console_putchar) as fallback;
     framebuffer initialisation same as x86-64 if present
2. Emit a startup banner identifying the kernel and the protocol version
3. Emit: CPU architecture identifier and core count if detectable at this stage
```

The early console is not the final console driver. It is a minimal, allocation-free
output path used only until userspace drivers take over. It has no input, no buffering,
and no colour support (beyond whatever the framebuffer pixel writer implements).

**Failure mode:** If no output device is found, initialisation continues silently.
This is not fatal — a headless system is valid.

**Completion criterion:** `arch::current::EarlyConsole::init()` has returned.

---

## Phase 2: Memory Map Parsing and Buddy Allocator

**What happens:**

The physical memory map from `BootInfo` is parsed to identify usable RAM. The buddy
allocator is initialised and all usable frames are added to it.

```
1. Iterate boot_info.memory_map.entries
2. For each entry with memory_type == MemoryType::Usable:
   a. Align start address up to PAGE_SIZE boundary
   b. Align end address down to PAGE_SIZE boundary
   c. Skip ranges smaller than PAGE_SIZE
   d. Add to candidate pool
3. Remove from the candidate pool:
   a. Frames containing the kernel image
      (boot_info.kernel_physical_base .. kernel_physical_base + kernel_size)
   b. Frames containing boot modules
      (boot_info.modules.entries[i].physical_base + size for each i)
   c. Frames containing the BootInfo structure itself
   d. Frames containing the bootloader's page tables (if identifiable)
4. Determine buddy allocator order range:
   - Minimum order: 0 (one 4 KiB page)
   - Maximum order: implementation constant, e.g. 10 (1024 pages = 4 MiB)
5. Call mm::buddy::BuddyAllocator::new(max_order) — this is a static or
   early-heap allocation using only the bootloader-provided stack
6. For each candidate range, call BuddyAllocator::add_region(phys_start, phys_end)
7. Emit: total usable RAM in MiB
```

The buddy allocator at this stage cannot allocate its own metadata using itself —
it must be initialised from a fixed-size static buffer or from the boot stack. The
allocator's free lists are small (one pointer per order level) and fit in a static
array.

**Memory at this point:** Only the buddy allocator metadata is allocated. No kernel
heap exists yet.

**Failure mode:** If total usable RAM is zero after exclusions, halt with message
"fatal: no usable physical memory". This indicates a corrupt memory map.

**Completion criterion:** `BuddyAllocator` is initialised and reports usable frames.

---

## Phase 3: Kernel Page Tables

**What happens:**

The kernel replaces the bootloader's minimal page tables with its own, establishing
the full virtual address space layout described in
[docs/memory-model.md](../../docs/memory-model.md).

```
1. Allocate a root page table frame via BuddyAllocator::alloc(order=0)
2. Zero the frame
3. Map the kernel image at its virtual addresses:
   - Text segment: readable, executable, not writable
   - Rodata segment: readable, not writable, not executable
   - Data/BSS segment: readable, writable, not executable
   (Segment addresses from ELF headers, sizes from BootInfo)
4. Map the direct physical map:
   - For each usable physical range: map at PHYSMAP_BASE + phys_addr
   - Use 2 MiB large pages where alignment permits
   - Use 1 GiB huge pages where alignment permits and range is large enough
   - Permissions: readable, writable, not executable
   PHYSMAP_BASE = 0xFFFF800000000000 (both architectures)
5. Map the BootInfo structure and boot modules (needed until they are consumed)
6. Install the new page table:
   arch::current::Paging::activate(&new_table, pcid=0)
   (PCID/ASID 0 is reserved for kernel-only contexts)
7. The bootloader page table is no longer referenced; its frames are not freed yet
   (they remain allocated but will be reclaimed after Phase 4)
8. Emit: "page tables established, physmap at 0xFFFF800000000000"
```

After this phase, the kernel can access any physical frame at `PHYSMAP_BASE + phys`.
All kernel pointers derived from physical addresses use this translation.

**Failure mode:** Frame allocation failure during page table construction is fatal.
Emit "fatal: cannot build kernel page tables (OOM)" and halt.

**Completion criterion:** The kernel is executing with its own page tables active.

---

## Phase 4: Slab Allocator and Kernel Heap

**What happens:**

The slab allocator and size-class allocator are initialised, enabling dynamic
allocation of kernel objects for the first time.

```
1. Initialise the general size-class allocator:
   - Bins at power-of-two sizes (exact range determined at implementation time)
   - Each bin backed by slab pages from the buddy allocator on demand
2. Register slab caches for core kernel objects:
   - CapabilitySlot (fixed size)
   - ThreadControlBlock (fixed size)
   - Endpoint (fixed size)
   - Signal (fixed size)
   - EventQueue header (fixed size; ring buffer body from size-class allocator)
   - WaitSet (fixed size)
   - AddressSpace (fixed size)
   - PageTableNode (fixed size; one per level-below-root page table frame)
3. Install the kernel allocator (implements the `GlobalAlloc` trait via the
   size-class path; used by any `alloc::*` usage in the kernel)
4. Reclaim bootloader page table frames (no longer needed)
5. Emit: "kernel heap active"
```

After this phase, `Box`, `Vec`, and other heap types work in kernel code.

**Failure mode:** If slab initialisation fails to allocate its first backing pages,
halt with "fatal: cannot initialise kernel heap".

**Completion criterion:** The kernel allocator is active and `alloc::boxed::Box`
allocations succeed.

---

## Phase 5: Architecture Hardware Initialisation

**What happens:**

Architecture-specific hardware structures are established. This is the phase where
x86-64 and RISC-V diverge most significantly.

### x86-64

```
1. Construct and install a permanent GDT:
   - Null descriptor (index 0)
   - Kernel code segment (64-bit, DPL 0)
   - Kernel data segment (DPL 0)
   - User data segment (DPL 3)
   - User code segment (64-bit, DPL 3)
   - TSS descriptor (per CPU)
2. For each CPU, construct a TSS:
   - RSP0: kernel stack pointer for privilege transitions
   - IST1..IST7: interrupt stack table entries (for NMI, double fault, etc.)
3. Construct and install the IDT:
   - Exception handlers for vectors 0–31 (divide error, page fault, etc.)
   - APIC timer vector (preemption)
   - Spurious interrupt vector
   - Syscall vector (though SYSCALL/SYSRET bypasses the IDT)
4. Enable SMEP and SMAP in CR4 if CPUID reports support
5. Configure SYSCALL/SYSRET:
   - Write kernel entry point to LSTAR MSR
   - Write segment selectors to STAR MSR
   - Write SFMASK to clear IF on entry
6. Initialise the local APIC on the BSP
7. Configure the APIC timer for preemption (period from scheduler policy)
8. Enable interrupts (STI)
```

### RISC-V

```
1. Write trap handler address to stvec (direct mode)
2. Configure sstatus:
   - Clear SIE (interrupts remain disabled until scheduler starts)
   - Clear SPP (so sret returns to U-mode by default)
   - Clear SUM (no supervisor access to user pages)
3. Enable SEIP, STIP in sie (external and timer interrupt enables)
4. Initialise PLIC for this hart: configure priorities and enables
5. Set SBI timer for initial tick (timer interrupt enable)
6. Enable interrupts (set sstatus.SIE)
```

**Failure mode:** Hardware initialisation failures (e.g. CPUID indicates a required
feature is absent) halt with a descriptive message. The specific required features
are checked against constants defined in `arch/x86_64/cpu.rs` and
`arch/riscv64/cpu.rs`.

**Completion criterion:** Interrupts are enabled, the preemption timer is running,
and the syscall entry mechanism is installed.

---

## Phase 6: Platform Resource Validation

**What happens:**

The `platform_resources` slice from `BootInfo` is validated before Phase 7 mints
capabilities from it. The kernel does not parse ACPI or Device Tree — the bootloader
has already done that and produced structured descriptors. The kernel's job here is
to reject entries that would be unsafe to use.

```
1. If platform_resources.count == 0: skip validation, proceed with empty set.
2. Verify platform_resources.entries is non-null (required when count > 0).
3. Verify the slice falls within boot-provided physical memory:
   - The entire range [entries, entries + count * size_of::<PlatformResource>())
     must be within regions the memory map marks as Usable or Loaded.
4. For each PlatformResource entry:
   a. Check resource_type is a known discriminant; skip with warning if not.
   b. For MmioRange, PciEcam, PlatformTable, IommuUnit:
      - Verify base is page-aligned; skip with warning if not.
      - Verify size > 0 and size is page-aligned; skip with warning if not.
      - Verify base + size does not wrap around; skip with warning if not.
   c. For IoPortRange (x86-64 only):
      - Verify base <= 0xFFFF; skip with warning if not.
      - Verify base + size <= 0x10000; skip with warning if not.
      - On RISC-V: skip all IoPortRange entries silently.
   d. For IrqLine:
      - Verify id is within the platform's interrupt number range; skip if not.
5. Check for overlapping MmioRange and PciEcam entries (overlaps within a type
   are invalid); skip the later entry with a warning.
6. Emit: "platform resources: N entries validated (M skipped)"
```

Validation failures are non-fatal at the entry level — bad entries are skipped
with a warning. The only fatal condition is a null `entries` pointer when `count > 0`,
which indicates a corrupt BootInfo.

**Failure mode:** Null `entries` when `count > 0`: halt with "fatal: platform_resources
pointer is null with non-zero count". Individual bad entries: emit a warning and skip.

**Completion criterion:** The validated platform resource list is available to Phase 7.

---

## Phase 7: Capability System

**What happens:**

The capability subsystem is initialised and the root CSpace (which will be given to
init) is created. Capabilities are minted from the validated `platform_resources`
produced in Phase 6.

```
1. Initialise the global derivation tree (initially empty)
2. Allocate the root CSpace:
   - Initial capacity: ROOT_CSPACE_INITIAL_SLOTS (e.g. 1024 slots)
   - Slot 0 is permanently null
3. Populate the root CSpace with initial capabilities:
   a. Frame capabilities for all usable physical memory ranges
      (one capability per contiguous usable region from the memory map)
   b. Capabilities from boot-provided platform resources (one per validated entry):
      - MmioRange entries → MmioRegion capabilities (Map rights)
      - IrqLine entries → Interrupt capabilities
      - PciEcam entries → MmioRegion capabilities (ECAM is an MMIO range)
      - PlatformTable entries → read-only Frame capabilities (Map rights only;
        no Write or Execute — these are firmware tables for userspace reading)
      - IoPortRange entries → IoPortRange capabilities (x86-64 only; Use rights)
      - IommuUnit entries → MmioRegion capabilities (for devmgr to configure DMA)
   c. One SchedControl capability (Elevate rights) — allows the holder to set
      thread priorities in the elevated range (21–30); delegated by init to
      services that require real-time-ish scheduling priority
   d. (Thread and process capabilities for init are added in Phase 9)
4. Record the root CSpace pointer in a global for use in Phase 9
5. Emit: "capability system initialised, N slots populated"
```

The initial capability population is the only point where capabilities are created
without a parent. All authority in the running system derives from this grant.

**Failure mode:** Allocation failure during CSpace construction halts with
"fatal: cannot initialise capability system".

**Completion criterion:** Root CSpace exists and contains capabilities for all
boot-provided hardware resources.

---

## Phase 8: Scheduler

**What happens:**

The per-CPU scheduler state is initialised. At this point no runnable threads exist;
the idle threads are created here to ensure each CPU always has something to run.

```
1. Initialise per-CPU run queues:
   - NUM_PRIORITY_LEVELS priority queues per CPU (e.g. 32 levels)
   - Each queue is an intrusive doubly-linked list of TCBs
2. For each CPU (including the BSP):
   a. Allocate a kernel stack (KERNEL_STACK_SIZE pages from buddy allocator)
   b. Allocate and initialise an idle TCB:
      - Priority: IDLE_PRIORITY (lowest, reserved; never preempted)
      - Entry: arch::current::Context::new_state(idle_entry, stack_top, cpu_id, false)
      - Idle thread entry calls Cpu::halt_until_interrupt() in a loop,
        checking for pending work before each halt
   c. Set the per-CPU current_thread pointer to the idle TCB
3. Emit: "scheduler initialised, N CPUs"
```

No scheduling decisions are made yet — the BSP continues executing init-sequence
code as the "current thread", which will transition to being the thread that spawns
init.

**Failure mode:** Allocation failure for any idle stack or TCB halts with
"fatal: cannot initialise scheduler".

**Completion criterion:** Per-CPU scheduler state and idle threads are initialised
for all CPUs.

---

## Phase 9: Init Process Creation

**What happens:**

The init binary (first boot module) is loaded and an initial process is created with
a fully populated CSpace.

```
1. Validate the init ELF:
   a. Check ELF magic bytes
   b. Verify e_machine matches the current architecture
   c. Verify e_type == ET_EXEC (static executable; no dynamic linking in init)
   d. Verify the entry point is within a LOAD segment
2. Create the init address space:
   a. Allocate a new page table (arch::current::Paging::new_page_table())
   b. Map the kernel higher half into the new address space (shared kernel mapping)
3. Load ELF LOAD segments into the init address space:
   a. Allocate physical frames for each segment (from buddy allocator)
   b. Copy segment data from the module's physical memory
   c. Zero BSS regions
   d. Map at the segment's virtual address with ELF-specified permissions
4. Allocate init's user stack:
   a. Allocate INIT_STACK_PAGES frames
   b. Map below INIT_STACK_TOP with read/write permissions
   c. Place a guard page (unmapped) immediately below the stack
5. Create the init TCB:
   a. Allocate a kernel stack for init (for syscall handling)
   b. arch::current::Context::new_state(entry=elf.e_entry, stack_top=INIT_STACK_TOP,
      arg=root_cspace_descriptor, is_user=true)
   c. Set priority to INIT_PRIORITY (high, but below real-time range)
6. Create process and thread capabilities for init:
   a. Allocate ProcessCap and ThreadCap objects
   b. Insert into the root CSpace (these are how the kernel refers to init)
7. Transfer the root CSpace to init:
   a. The CSpace created in Phase 7 becomes init's CSpace
   b. Init receives its own thread and process capabilities in well-known slots
   c. Remaining slots hold the hardware capabilities from Phase 7
8. Enqueue the init TCB on the BSP's run queue at INIT_PRIORITY
9. Reclaim boot module frames (init binary is now loaded; original mapping unneeded)
10. Emit: "init process created, entry at 0x{entry:016x}"
```

**Failure mode:** ELF validation failure, allocation failure, or inability to map
segments halts with a diagnostic message indicating which step failed.

**Completion criterion:** Init TCB is enqueued and ready to run.

---

## Phase 10: Scheduler Handoff and SMP Bringup

**What happens:**

The BSP transfers control to the scheduler, which selects init (the highest-priority
runnable thread) and begins execution. Secondary CPUs are brought up in parallel.

```
BSP:
1. Emit: "entering scheduler"
2. Call sched::enter() — this does not return
   sched::enter() selects the highest-priority runnable thread (init),
   calls arch::current::Context::return_to_user(&init_tcb.saved_state)
   Init begins executing at its ELF entry point.

Secondary CPUs (in parallel with init startup on BSP):
3. Each secondary CPU is signalled:
   - x86-64: BSP sends INIT+SIPI to each AP's APIC ID
   - RISC-V: BSP calls SBI HSM hart_start for each secondary hart
4. Secondary CPUs execute a minimal AP/hart startup stub:
   a. Load the kernel stack pointer (pre-allocated in Phase 8)
   b. Install the kernel page table (same root as BSP)
   c. Call arch::current::Cpu::init_local() for per-CPU state
   d. Call arch::current::Interrupts::init()
   e. Call arch::current::Timer::init(TIMER_PERIOD_US)
   f. Call arch::current::Syscall::init()
   g. Call sched::enter() — begins running the idle thread for this CPU
5. BSP waits for all secondary CPUs to reach step 4g before declaring SMP active
   (a shared atomic counter is incremented by each secondary on entry to sched::enter)
6. Emit (from BSP after all secondaries are up): "SMP: N CPUs online"
```

After this phase, the kernel is fully operational. Init runs in userspace and begins
its service startup sequence. The kernel only executes when a syscall or interrupt
brings a CPU into kernel mode.

**Failure mode:** If a secondary CPU fails to come up within a timeout, a warning is
emitted and that CPU is marked offline. Loss of secondary CPUs is not fatal — the
system can operate on fewer CPUs. Failure of the BSP to enter the scheduler is fatal.

**Completion criterion:** Init is running in userspace; all secondary CPUs are in
their idle loops.

---

## Fatal Boot Failure Handling

At any phase, if the kernel cannot continue:

```rust
fn fatal(msg: &str) -> !
{
    // Disable interrupts to prevent re-entrant failure handling.
    arch::current::Interrupts::disable();
    arch::current::EarlyConsole::write_str("KERNEL FATAL: ");
    arch::current::EarlyConsole::write_str(msg);
    arch::current::EarlyConsole::write_str("\n");
    loop
    {
        // Halt until the next interrupt (hlt on x86-64; wfi on RISC-V).
        // Interrupts are left disabled — this CPU is not taking further work.
        arch::current::Cpu::halt_until_interrupt();
    }
}
```

Secondary CPU failures after Phase 10 are handled by `fatal()` on that CPU only; the
BSP and other CPUs continue.

---

## Initialization Summary

| Phase | Key Action | Failure |
|---|---|---|
| 0 | Validate BootInfo version | Silent halt |
| 1 | Early console | Non-fatal (continues silently) |
| 2 | Buddy allocator from memory map | Halt: no usable RAM |
| 3 | Kernel page tables + direct map | Halt: OOM during PT construction |
| 4 | Slab allocator + kernel heap | Halt: cannot init heap |
| 5 | CPU hardware (IDT/GDT/TSS/stvec) | Halt: missing required feature |
| 6 | Platform resource validation | Halt if entries pointer is null with non-zero count; bad entries skipped |
| 7 | Capability system + root CSpace | Halt: OOM |
| 8 | Scheduler + idle threads | Halt: OOM |
| 9 | Init process creation | Halt: invalid ELF or OOM |
| 10 | Scheduler handoff + SMP bringup | AP failure: warning; BSP failure: halt |
