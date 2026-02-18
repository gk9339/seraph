# Seraph

Seraph is a microkernel operating system written in Rust, targeting x86-64 and RISC-V (RV64GC).

## Goals

- Minimal, modular microkernel — maximum code in userspace
- Written primarily in Rust; architecture-specific assembly isolated, documented, and behind shared trait abstractions
- Capability-based security model throughout
- Clear component boundaries with well-defined IPC contracts
- Multi-architecture support: x86-64 and RISC-V (RV64GC)
- Support for modern hardware features: multi-core execution (SMP), SMT-aware scheduling, per-process TLB tagging (PCIDs/ASIDs), and IOMMU-enforced device isolation
- Self-hosting as a long-term goal

No binary compatibility with other operating systems.
No support for 32-bit or legacy x86.

## Structure

| Directory | Purpose |
|---|---|
| `boot/` | UEFI bootloader |
| `kernel/` | Microkernel (scheduler, IPC, memory, capabilities) |
| `libc/` | C standard library |
| `init/` | Service manager / PID 1 |
| `devmgr/` | Device manager (platform enumeration, driver binding) |
| `vfs/` | Virtual filesystem server |
| `net/` | Network stack server |
| `drivers/` | Userspace device drivers |
| `base/` | General-purpose userspace applications and utilities |
| `docs/` | Architecture and design documentation |
| `scripts/` | Build system, tooling, and helper scripts |

## Documentation

Design documents live in [`docs/`](docs/):

- [Architecture Overview](docs/architecture.md) — component structure and design philosophy
- [Memory Model](docs/memory-model.md) — virtual address space layout, paging, allocation
- [IPC Design](docs/ipc-design.md) — message passing, endpoints, synchronous vs async
- [Capability Model](docs/capability-model.md) — permissions, delegation, revocation
- [Boot Protocol](docs/boot-protocol.md) — UEFI boot flow, boot info contract, kernel entry requirements
- [Device Management](docs/device-management.md) — platform enumeration, devmgr, driver binding, DMA safety
- [Coding Standards](docs/coding-standards.md) — Rust conventions, safety contracts, documentation rules
