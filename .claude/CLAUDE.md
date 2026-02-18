# Seraph — AI Context

Seraph is a microkernel OS in Rust targeting x86-64 and RISC-V (RV64GC).
See @../README.md for project goals, structure, and component overview.

## Project Stage

This project is in the design-document phase. The docs in `docs/` and `kernel/docs/`
are the source of truth. No production source code exists yet. When writing code,
implement what the design documents specify — do not invent new interfaces or restructure
without consulting the relevant document first.

## Coding Standards

Read @../docs/coding-standards.md before writing or reviewing any code. The rules most
commonly violated by AI assistants:

- **Allman brace style**: opening brace on its own line, always
- **`} else`**: `else` stays on the same line as the closing brace; the opening brace
  goes on the next line (e.g. `} else\n{`)
- **100-column line limit**
- **No `unwrap()`/`expect()`/`panic!()`** in production code paths
- **Every `unsafe` block** requires a preceding `// SAFETY:` comment explaining why the
  operation is sound

## Architectural Invariants

These apply everywhere in the codebase. Violating any of them produces a fundamentally
wrong design.

1. **Microkernel boundary is strict.** The kernel handles IPC, scheduling, memory
   management, and capabilities. Drivers, filesystems, networking, device enumeration,
   and firmware parsing all live in userspace. Do not suggest moving functionality into
   the kernel for convenience or performance without strong justification.

2. **Capabilities are the sole access control mechanism.** There is no ambient authority,
   no UID/GID, no root/superuser at the kernel level. Every resource operation requires
   a capability. See @../docs/capability-model.md.

3. **No `#[cfg(target_arch)]` outside `arch/` modules.** Architecture-specific code
   lives behind trait boundaries in `arch/` directories. New arch-dependent behaviour
   goes into the trait, not into a cfg guard in shared code.

4. **W^X is enforced everywhere.** No page is simultaneously writable and executable.
   No capability may hold both Write and Execute rights.

5. **All allocation is fallible.** Every call site that allocates memory handles failure
   explicitly. No OOM killer, no silent failure, no unwrap on allocation results.

6. **IPC is message-passing, not shared memory by default.** Shared memory is a
   deliberate optimisation established via capability grants, not the default.

7. **Not POSIX.** Seraph defines its own native interfaces. Do not suggest `fork()`,
   UNIX signals, or POSIX-style APIs. No binary compatibility with other operating
   systems.

## Design Documents

Consult the relevant document before making architectural suggestions:

- @../docs/architecture.md — system structure, component diagram, design philosophy
- @../docs/memory-model.md — address space layout, paging, TLB, allocator overview
- @../docs/ipc-design.md — sync IPC, signals, event queues, wait sets
- @../docs/capability-model.md — capability types, rights, derivation, revocation
- @../docs/boot-protocol.md — UEFI boot flow, BootInfo contract, kernel entry state
- @../docs/device-management.md — devmgr, driver binding, DMA safety model
- @../docs/coding-standards.md — formatting, naming, error handling, unsafe rules

## Common Mistakes to Avoid

- Do not put driver logic, filesystem code, or protocol stacks in the kernel
- Do not suggest ACPI or Device Tree parsing inside the kernel (the bootloader does it)
- Do not design for x86-64 only; every feature needs a RISC-V story
- Do not hold locks across IPC calls or any operation that may block
- Do not use global mutable state without an explicit synchronisation primitive
- Capability **transfer** moves a cap (sender loses it); capability **derivation** creates
  a child with equal or fewer rights (sender retains the original). Do not confuse them.
