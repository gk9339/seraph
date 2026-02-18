# Architecture Overview

## Philosophy

Seraph is a microkernel. The kernel's role is to be a minimal, trusted foundation —
nothing more. It provides the primitives that make an operating system possible:
isolation, communication, scheduling, and resource control. All policy, all device
support, and all services are implemented in userspace.

This boundary is strict by design. Moving something into the kernel for convenience
or performance must be justified against the cost of expanding the trusted computing
base. The trusted computing base should remain as small as possible, because every
line of kernel code is a line that can corrupt the entire system if wrong.

---

## Kernel Responsibilities

The kernel implements exactly four things:

**IPC** — message passing between processes. The kernel delivers messages, manages
endpoints, and delivers asynchronous notifications. It enforces that communication
only occurs through authorised channels. It has no opinion on the content of messages.

**Scheduling** — preemptive, priority-based scheduling across all available CPUs.
The scheduler is SMT-aware and understands physical core topology. Userspace may
provide priority and affinity hints; policy decisions remain with the kernel.

**Memory management** — physical frame allocation, virtual address space management,
and page table maintenance. The kernel enforces isolation between address spaces.
All memory access between processes is explicit and capability-controlled.

**Capabilities** — the kernel's access control mechanism. Every resource —
memory regions, IPC endpoints, interrupt lines — is represented as a capability.
Without a capability, a process cannot interact with a resource. The kernel enforces
this unconditionally. See [capability-model.md](capability-model.md) for the full model.

The kernel does not implement: filesystems, device drivers, network stacks, user
management, or any policy. These live in userspace.

---

## System Architecture

```
  ┌─────────────────────────────────────────────────────────────┐
  │  Applications                                               │
  │  (gksh, editor, user programs, network utilities, ...)      │
  └──────────────────────┬──────────────────────────────────────┘
                         │
  ┌──────────────────────┴──────────────────────────────────────┐
  │  System Services                                            │
  │                                                             │
  │   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐     │
  │   │     vfs     │   │     net     │   │   drivers   │     │
  │   └─────────────┘   └─────────────┘   └─────────────┘     │
  └──────────────────────┬──────────────────────────────────────┘
                         │
  ┌──────────────────────┴──────────────────────────────────────┐
  │  init (PID 1) — service manager                             │
  └──────────────────────┬──────────────────────────────────────┘
                         │
  ╔══════════════════════╧══════════════════════════════════════╗
  ║  SERAPH KERNEL                                              ║
  ║  IPC  |  Scheduler  |  Memory Management  |  Capabilities  ║
  ╚══════════════════════╤══════════════════════════════════════╝
                         │
  ┌──────────────────────┴──────────────────────────────────────┐
  │  Hardware                                                   │
  │  (CPU, RAM, IOMMU, interrupt controllers, devices)          │
  └─────────────────────────────────────────────────────────────┘
```

All inter-component communication crosses the kernel via IPC. There are no shared
memory shortcuts between services except where explicitly established as a
capability-granted shared mapping.

---

## Userspace Services

**init** is the first userspace process, spawned directly by the kernel at the end
of boot. It is the service manager and the ancestor of all other processes. It reads
a boot configuration, starts system services in dependency order, and supervises them.
See [boot-protocol.md](boot-protocol.md) for how the kernel hands off to init.

**drivers** hosts device driver processes. Each driver runs in its own isolated
address space. Drivers access hardware via MMIO regions mapped into their address
space by the kernel under capability control (see Driver Model below). Interrupt
lines are delivered to drivers as asynchronous IPC notifications. No driver code
runs in kernel space.

**vfs** is the virtual filesystem server. It provides a unified namespace over
multiple underlying filesystems. Filesystem implementations (ext2, FAT, etc.) run
as separate processes within or alongside vfs, communicating via IPC. Block device
access goes through the appropriate driver.

**net** is the network stack server. It manages network interfaces (via driver IPC),
implements the protocol stack, and exposes socket-like endpoints to applications.

**base** contains general-purpose userspace programs: terminal emulator, shell (gksh),
text editor, coreutils equivalents, and network utilities. These are applications,
not services — they have no special privileges beyond what their capabilities grant.

---

## Driver Model

Drivers run as unprivileged userspace processes. Hardware access works as follows:

**MMIO:** The kernel maps the physical MMIO region for a device into the driver's
virtual address space, gated by a capability. Once mapped, the driver reads and
writes hardware registers directly with no kernel involvement. This is fast —
no syscall per register access — and is the primary hardware access mechanism.

**Port I/O (x86 only):** x86 port I/O (`in`/`out` instructions) cannot be
memory-mapped. The kernel grants a driver access to specific port ranges via the
I/O Permission Bitmap (IOPB) in the TSS. This allows the driver to execute port
I/O instructions directly, without a syscall, for its authorised port range only.
RISC-V has no port I/O concept and does not need this mechanism.

**DMA:** Drivers that perform DMA must have their physical access ranges authorised
by the IOMMU (x86: VT-d; RISC-V: IOMMU extension). The kernel programs the IOMMU
when granting DMA capabilities. A driver cannot DMA outside its authorised regions
even if its process is compromised.

**Interrupts:** Hardware interrupts are not delivered directly to drivers. The kernel
receives the interrupt, masks it, and delivers an asynchronous IPC notification to
the registered driver endpoint. The driver handles it and re-enables the line via
a syscall when ready.

---

## IPC Overview

IPC is the backbone of the system. All service requests, device events, and
inter-process communication go through the kernel's IPC mechanism.

Seraph uses a **hybrid model**:

- **Synchronous calls** for structured request/reply between services. The caller
  sends a message to an endpoint and blocks until the server replies. This gives
  simple call/return semantics for service interfaces.

- **Asynchronous notifications** for events — hardware interrupts, completion
  signals, and any case where the sender must not block. Notifications are
  non-blocking and do not carry a payload beyond a signal.

A process can perform a blocking wait on a set of endpoints and notifications,
returning when any of them become ready. This covers event-driven and
multiplexed I/O patterns.

Full IPC design, message format, endpoint lifecycle, and capability-passing
semantics are documented in [ipc-design.md](ipc-design.md).

---

## Memory Model Overview

The kernel occupies the higher half of the virtual address space on both
architectures. Each userspace process has its own isolated address space.
Physical memory is managed by the kernel's frame allocator; virtual mappings
are managed per address space. The kernel enforces W^X (no page is simultaneously
writable and executable) at the page table level.

Full virtual address space layout, frame allocator design, and heap management
are documented in [memory-model.md](memory-model.md).

---

## Capability Model Overview

Every resource in Seraph — memory regions, IPC endpoints, interrupt lines, MMIO
ranges, CPU time — is represented as a capability. A process can only interact
with a resource if it holds a valid capability for it. Capabilities can be
delegated to child processes and revoked by their issuer.

The capability system is the sole access control mechanism. There is no separate
permission layer. Full design is in [capability-model.md](capability-model.md).

---

## Target Platforms

**x86-64:** Primary target. Supported environments include physical hardware
and hosted execution under hypervisors. Modern x86-64 features — APIC, SMEP,
SMAP, PCIDs, VT-d IOMMU — are used where available. Legacy x86 (32-bit, i686
and below) is not supported.

**RISC-V (RV64GC):** Explicit second target. Initially developed and tested under
QEMU. Real RISC-V hardware is a future goal. The RV64GC profile (IMAFD +
compressed instructions) is assumed; more minimal embedded profiles are not targeted.

Architecture-specific code is isolated behind traits and module boundaries.
No architecture-neutral code contains `#[cfg(target_arch)]` guards. Adding a
new architecture means implementing the defined interfaces, not scattering
conditionals through shared code.

---

## Non-Goals

**POSIX API compatibility.** POSIX was designed around monolithic kernel assumptions.
`fork()`, signals, and related APIs are a poor fit for a capability-based microkernel.
Seraph defines its own native interfaces. Filesystem formats and network protocols
are adopted where useful as data formats, not as API commitments.

**Binary compatibility with other operating systems.** Seraph does not aim to run
Linux or other OS binaries.

