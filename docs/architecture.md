# Architecture Overview

## Philosophy

Seraph is a microkernel‑based operating system. The kernel is a minimal, trusted
component that provides only core mechanisms: isolation, communication, scheduling,
memory management, and capability enforcement.

All policy, device support, and system services live in userspace. The kernel does
not implement protocols or higher‑level abstractions; it enforces boundaries and
provides primitives that userspace composes into a complete system.

This boundary is strict by design. Expanding kernel functionality increases the
trusted computing base and the impact of failure, and must be treated as an
architectural decision rather than an implementation shortcut.

---

## Kernel Responsibilities

The kernel provides only the core mechanisms required to support the system. It
implements no policy and does not interpret higher‑level abstractions.

The kernel is responsible for:

**IPC**
Message delivery between processes, including endpoint management and asynchronous
notifications. The kernel enforces that communication occurs only via authorised
capabilities and does not interpret message contents.

**Scheduling**
Preemptive, priority‑based scheduling across all CPUs. Userspace may freely alter
priority to some level; changes beyond a certain level require explicit authority
via capabilies.

**Memory management**
Physical frame allocation, virtual address space management, and page table
maintenance. The kernel enforces isolation between address spaces and explicit,
capability‑controlled sharing.

**Capabilities**
The sole access control mechanism. All resources—memory regions, IPC endpoints,
interrupt lines, and CPU time—are represented as capabilities and enforced
unconditionally by the kernel. See [capability-model.md](capability-model.md)
for the full model.

The kernel does not implement filesystems, device drivers, network stacks, user
management, or other policy. These components run in userspace.

---

## System Architecture

All inter-component communication crosses the kernel via IPC. There are no shared
memory shortcuts between services except where explicitly established as a
capability-granted shared mapping.

---

## Userspace Services

System functionality beyond core mechanisms is implemented in userspace as isolated
services and applications. All services communicate exclusively via IPC and operate
under explicit capability grants.

**init**
The first userspace process, started by the kernel at the end of boot. init is a
minimal bootstrapper: it starts procmgr using a built-in ELF parser, then requests
that procmgr start the remaining early services (devmgr, svcmgr, drivers, VFS),
delegates all capabilities, and exits. init does not supervise services and is not
long-lived. See `boot-protocol.md`.

**procmgr**
Userspace process lifecycle manager, started by init. procmgr handles all subsequent
process creation, ELF loading, and teardown. No process is created after early boot
without going through procmgr (except svcmgr restarting procmgr itself).

**svcmgr**
Service health monitor, started by init before init exits. svcmgr monitors running
services, detects crashes, and requests restarts through procmgr. svcmgr holds raw
process-creation syscall capabilities as a fallback to restart procmgr if procmgr
crashes.

**devmgr**
The device manager, launched during bootstrap. devmgr receives platform resource
capabilities and read‑only access to firmware tables from init, enumerates devices,
spawns driver processes, and delegates per‑device capabilities. See `device-management.md`.

**drivers**
Device drivers run as isolated userspace processes. Each driver accesses hardware only
through capabilities granted by devmgr and the kernel. No driver code executes in
kernel space.

**vfs**
The virtual filesystem server. vfs provides a unified namespace over one or more
filesystem implementations, which may run as separate processes. Block device access
is mediated via driver IPC.

**net**
The network stack server. net manages network interfaces via driver IPC, implements
network protocols, and exposes socket‑like interfaces to applications.

**base**
General‑purpose userspace applications and utilities (shell, terminal, editor,
core tools). These are unprivileged applications with no authority beyond their
capabilities.

---

## Kernel Primitives vs. Userspace Abstractions

The kernel manages three primitive object types:

- **Thread** — a schedulable unit of execution with a saved register state, a priority,
  and bindings to an AddressSpace, a CSpace, and an IPC buffer.
- **AddressSpace** — a virtual address space with a page table root and a set of
  frame mappings. Revoking an AddressSpace capability stops all threads bound to it.
- **CSpace** — a capability space: a growable array of capability slots that a thread
  uses to name kernel objects.

The kernel has no "Process" object. A **process** is a userspace convention: a group
of threads sharing an AddressSpace and a CSpace, managed by procmgr. The kernel
enforces isolation via AddressSpace and CSpace boundaries, not via a process abstraction.

---

## Bootstrap Sequence

```
Bootloader → kernel_entry
    → Phase 0–8: memory, paging, caps, scheduler init
    → Phase 9: map init from pre-parsed InitImage segments
    → Phase 10: scheduler handoff — init runs in userspace

init
    → start procmgr (built-in ELF parser + raw syscalls)
    → request: start devmgr, svcmgr, drivers, VFS [, net]
    → delegate capabilities to each service
    → exit

procmgr    — all subsequent process creation goes through here
devmgr     — platform enumeration, driver binding
drivers    — block, FS, [net], etc.
VFS        — unified filesystem namespace
svcmgr     — service monitoring, restarts, procmgr fallback
[net]      — network stack (optional early-boot module)
```

Boot modules (procmgr, devmgr, drivers, etc.) are configurable via `boot.conf`.
The bootloader loads the configured set from the ESP; init starts them in order.
The minimum set is: procmgr, devmgr, one block driver, one FS driver, VFS.

---

## Driver Model

Device drivers run as unprivileged userspace processes. No driver code executes in
kernel space. Hardware access is granted explicitly via capabilities and is fully
revocable.

**MMIO**
Physical MMIO regions are mapped into a driver’s address space under capability
control. Once mapped, drivers access registers directly without kernel mediation.

**Port I/O (x86‑64 only)**
Drivers receive an IoPortRange capability for assigned port ranges. Binding this
capability enables direct execution of port I/O instructions for those ranges.
Access is revoked automatically when the capability is revoked. RISC‑V does not
support port I/O.

**DMA**
DMA access requires an explicit DMA capability. On platforms with an IOMMU, the
kernel programs the IOMMU to restrict DMA to authorised regions. On platforms
without an IOMMU, DMA isolation is not enforced; callers must explicitly acknowledge
this when granting DMA access. See `device-management.md`.

**Interrupts**
Hardware interrupts are received by the kernel and delivered to drivers as
asynchronous IPC notifications. Drivers re‑enable interrupt delivery explicitly
after handling.

---

## IPC Overview

All inter‑process communication in Seraph occurs via the kernel’s IPC mechanism.
There are no implicit shared‑memory shortcuts; any shared memory is established
explicitly via capability‑granted mappings.

Seraph uses a hybrid IPC model:

- **Synchronous calls** for structured request/reply interactions between services.
- **Asynchronous notifications** for events such as interrupts and completion signals.

Processes may block on a set of endpoints and notifications, enabling event‑driven
and multiplexed I/O patterns.

Full IPC semantics, message formats, endpoint lifecycle rules, and capability‑passing
behavior are defined in [ipc-design.md](ipc-design.md).

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


Capabilities are the sole access control mechanism in Seraph. Every resource—
memory regions, IPC endpoints, interrupt lines, and CPU time—is represented by a
capability and enforced by the kernel.

A thread may interact with a resource only if it holds a valid capability for it.
Capabilities may be delegated and revoked by their issuer. There is no separate
permission or identity-based authority model.


The complete capability design, including delegation, revocation, and lifetime
rules, is defined in [capability-model.md](capability-model.md).

---

## Target Platforms

Seraph targets 64‑bit architectures with modern MMU and privilege support.

**x86‑64**
Seraph supports the x86‑64 architecture and makes use of contemporary architectural
features where available (e.g. APIC, PCIDs, IOMMU). Legacy x86 variants (32‑bit and
earlier) are not supported.

**RISC‑V (RV64GC)**
Seraph supports the RISC‑V 64‑bit architecture with the RV64GC base ISA and standard
extensions (IMAFD with compressed instructions). More minimal or embedded‑focused
configurations are not targeted.

See [coding-standards.md#architecture-specific-code](coding-standards.md#architecture-specific-code)
for the architectural code isolation rules.

---

## Non-Goals

**POSIX API compatibility.**
POSIX was designed around monolithic kernel assumptions.
`fork()`, signals, and related APIs are a poor fit for a capability-based microkernel.
Seraph defines its own native interfaces. Filesystem formats and network protocols
are adopted where useful as data formats, not as API commitments.

**Binary compatibility with other operating systems.**
Seraph does not aim to run Linux or other OS binaries.

