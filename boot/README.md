# boot

UEFI bootloader for Seraph. Reads boot configuration from `\EFI\seraph\boot.conf`,
loads the kernel ELF and boot modules, parses init's ELF into `InitImage`, establishes
initial page tables with W^X enforcement, discovers firmware table addresses (ACPI
RSDP / Device Tree blob) for passthrough to userspace, and jumps to the kernel entry
point.

The boot protocol contract — CPU state at entry, `BootInfo` structure layout, and
`PlatformResource` format — is documented in
[docs/boot-protocol.md](../docs/boot-protocol.md).

The shared protocol types live in [`shared/boot-protocol/`](../shared/boot-protocol/).

---

## Source Layout

```
boot/
├── Cargo.toml                  # seraph-boot crate (UEFI application)
├── linker/
│   └── riscv64-uefi.ld         # Linker script for RISC-V PE/COFF pipeline
└── src/
    ├── main.rs                 # efi_main — boot sequence orchestrator
    ├── config.rs               # Boot configuration file parser (boot.conf)
    ├── uefi.rs                 # UEFI protocol wrappers and memory services
    ├── elf.rs                  # ELF parser, segment loader, entry point extraction
    ├── firmware.rs             # ACPI / Device Tree address discovery
    ├── paging.rs               # Initial page table construction (arch-neutral)
    ├── error.rs                # Bootloader error type
    └── arch/
        ├── mod.rs              # Re-exports the active arch module
        ├── x86_64/
        │   └── paging.rs       # x86-64 4-level page table implementation
        └── riscv64/
            ├── paging.rs       # RISC-V Sv48 page table implementation
            └── header.S        # Hand-crafted PE32+ header and entry trampoline
```

---

## Crate Structure

**`boot-protocol`** (`shared/boot-protocol/`) — a `no_std` crate with no dependencies.
Defines `BootInfo` and all associated types as a stable `#[repr(C)]` interface shared
between the bootloader and the kernel. Also exports the `BOOT_PROTOCOL_VERSION`
constant. Neither crate links to the other; both depend on `boot-protocol` as a
workspace member.

**`seraph-boot`** (`boot/`) — the UEFI application. Depends on `boot-protocol` for the
`BootInfo` type it populates. Architecture-specific code is isolated to `arch/*/`;
no `#[cfg(target_arch)]` guards appear in the shared modules (`uefi.rs`, `elf.rs`,
`firmware.rs`, `paging.rs`).

---

## Build

The bootloader is built as part of the Seraph workspace. Refer to
[scripts/README.md](../scripts/README.md) for the full build procedure. Key points:

| Architecture | Target triple | Output |
|---|---|---|
| x86-64 | `x86_64-unknown-uefi` | `.efi` (PE/COFF, direct from linker) |
| RISC-V | `riscv64gc-seraph-uefi` | `.efi` (flat binary via `llvm-objcopy`) |

On x86-64, the Rust toolchain emits a PE/COFF `.efi` directly. On RISC-V, LLVM has
no PE/COFF backend, so the output ELF is converted to a flat binary with a
hand-crafted header prepended. See [docs/riscv-uefi-boot.md](docs/riscv-uefi-boot.md)
for details.

---

## Documentation

| Document | Content |
|---|---|
| [docs/boot-protocol.md](../docs/boot-protocol.md) | Boot contract: CPU state, `BootInfo` layout, `PlatformResource` format |
| [docs/boot-flow.md](docs/boot-flow.md) | Ten-step boot sequence, `BootInfo` population, kernel handoff |
| [docs/uefi-environment.md](docs/uefi-environment.md) | UEFI protocols, memory allocation, `ExitBootServices`, error handling |
| [docs/elf-loading.md](docs/elf-loading.md) | ELF validation, LOAD segment processing, boot module loading |
| [docs/firmware-parsing.md](docs/firmware-parsing.md) | ACPI and Device Tree → `PlatformResource` extraction |
| [docs/page-tables.md](docs/page-tables.md) | Initial page table construction for x86-64 and RISC-V |
| [docs/riscv-uefi-boot.md](docs/riscv-uefi-boot.md) | RISC-V PE/COFF workaround: header, linker script, build pipeline |

---

## Entry Point

`efi_main` in `src/main.rs` is the UEFI application entry point, declared
`extern "efiapi"`. UEFI firmware calls it with `(image_handle, system_table)` after
loading and relocating the image. It does not return; the final act is a one-way jump
to `kernel_entry` in the kernel binary.

The CPU state established at the kernel entry point is specified in
[docs/boot-protocol.md](../docs/boot-protocol.md).

---

## What the Bootloader Does Not Do

- **No UEFI runtime services.** UEFI is fully exited before the kernel runs.
- **Shallow firmware parsing only.** The bootloader records ACPI RSDP and Device
  Tree blob addresses in `BootInfo` and extracts structured `PlatformResource`
  entries from ACPI/MADT/MCFG and Device Tree nodes. Full namespace evaluation
  and driver binding are `devmgr`'s responsibility.
- **No PCI enumeration.** PCI ECAM windows are recorded as `PciEcam` entries; the
  bus scan is deferred to userspace.
- **No boot menu or interactive UI.** File paths come from `boot.conf`; the kernel
  command line is an opaque string passed through to `BootInfo`.
- **No permanent page tables.** The initial tables are minimal and temporary; the
  kernel replaces them during Phase 3 of its initialisation sequence.

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/architecture.md](../docs/architecture.md) | System-wide design philosophy and microkernel boundary |
| [docs/memory-model.md](../docs/memory-model.md) | Virtual address space layout the bootloader must establish |
| [docs/capability-model.md](../docs/capability-model.md) | Initial capabilities minted from `PlatformResource` entries |
| [docs/device-management.md](../docs/device-management.md) | How `devmgr` uses the resources the bootloader provides |
| [docs/coding-standards.md](../docs/coding-standards.md) | Formatting, naming, safety rules |
