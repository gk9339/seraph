# Seraph

Seraph is a microkernel operating system written in Rust, targeting x86-64 and RISC-V (RV64GC).

## Goals

- Minimal, modular microkernel; most functionality in userspace
- Capability-based security model throughout
- Clear component boundaries with explicit IPC contracts
- Architecture-specific code isolated behind shared traits
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

Overall project design documents live in [`docs/`](docs/):

- [Architecture Overview](docs/architecture.md) — component structure and design philosophy
- [Memory Model](docs/memory-model.md) — virtual address space layout, paging, allocation
- [IPC Design](docs/ipc-design.md) — message passing, endpoints, synchronous vs async
- [Capability Model](docs/capability-model.md) — permissions, delegation, revocation
- [Boot Protocol](docs/boot-protocol.md) — UEFI boot flow, boot info contract, kernel entry requirements
- [Device Management](docs/device-management.md) — platform enumeration, devmgr, driver binding, DMA safety
- [Coding Standards](docs/coding-standards.md) — Rust conventions, safety contracts, documentation rules

Each module contains a `README.md` that references the design docs relevant to that module.
