# scripts/

Build system scripts and tooling for Seraph.

---

## Directory Contents

| Path | Purpose |
|---|---|
| `env.sh` | Shared environment variables and helper functions; sourced by other scripts |
| `targets/x86_64-seraph-none.json` | Custom Rust target spec for the x86-64 kernel |
| `targets/riscv64gc-seraph-none.json` | Custom Rust target spec for the RISC-V kernel |
| `targets/riscv64gc-seraph-uefi.json` | Custom Rust target spec for the RISC-V bootloader (PIC ELF, converted to PE32+ via objcopy) |

---

## Root-Level Scripts

The following scripts live in the repository root for convenience:

### `build.sh`

```
./build.sh [--arch x86_64|riscv64] [--release] [--component boot|kernel|all]
```

Builds the specified component(s) and copies artifacts to the sysroot. Defaults to
`x86_64` debug, all components.

The sysroot is architecture-specific: building for a different arch than the existing
sysroot is an error. Run `./clean.sh` first when switching architectures.

| Option | Default | Description |
|---|---|---|
| `--arch` | `x86_64` | Target architecture |
| `--release` | off | Build in release mode |
| `--component` | `all` | `boot`, `kernel`, or `all` |

### `clean.sh`

```
./clean.sh [--all]
```

Removes `sysroot/`. With `--all`, also runs `cargo clean` to remove `target/`.

### `run.sh`

```
./run.sh [--arch x86_64|riscv64] [--release] [--no-build] [--gdb]
```

Builds (unless `--no-build`) then launches QEMU. Always boots via the UEFI
bootloader — direct kernel loading is not supported. The sysroot is passed
directly to QEMU as a virtual FAT drive; no temporary copies are made.

| Option | Description |
|---|---|
| `--arch` | Target architecture (default: `x86_64`) |
| `--release` | Use release build |
| `--no-build` | Skip the build step |
| `--gdb` | Start QEMU with a GDB server on localhost:1234; QEMU pauses until a debugger connects |

**x86-64** uses KVM acceleration unconditionally and requires OVMF firmware.
**RISC-V** uses TCG emulation (no KVM) with edk2 UEFI firmware and OpenSBI
(loaded automatically by QEMU's `virt` machine).

| Distro | x86-64 | RISC-V |
|---|---|---|
| Arch Linux | `pacman -S edk2-ovmf qemu-system-x86` | `pacman -S qemu-system-riscv` + AUR: `edk2-riscv` |
| Ubuntu / Debian | `apt install ovmf qemu-system-x86` | `apt install qemu-efi-riscv64 qemu-system-misc` |
| Fedora | `dnf install edk2-ovmf qemu-system-x86` | `dnf install edk2-riscv64 qemu-system-riscv` |

The bootloader is compiled as a PIC ELF and converted to a flat PE32+ binary
via `llvm-objcopy -O binary`. The `llvm-tools` rustup component (listed in
`rust-toolchain.toml`) provides the required `llvm-objcopy`.

---

## Environment Variables

Scripts respect these environment variables as overridable defaults:

| Variable | Default | Description |
|---|---|---|
| `SERAPH_ARCH` | `x86_64` | Default target architecture |

---

## Custom Targets

The JSON files in `targets/` define custom Rust compilation targets for bare-metal
kernel builds. Standard Rust targets are not suitable because the kernel requires:

- **x86-64**: Red zone disabled (required for interrupt safety), SSE/AVX/MMX
  disabled (kernel must not use SIMD before explicit init), kernel code model
  (for higher-half placement), static relocation.
- **RISC-V**: RV64GC instruction set, medium code model, lp64d ABI, static relocation.

Both targets set `panic-strategy: abort` and link with the bundled `rust-lld`.
They require `-Zbuild-std=core,alloc,compiler_builtins` (passed by the build scripts).

---

## Adding a New Architecture

1. Add a `scripts/targets/<triple>.json` following the existing specs
2. Add cases for the new arch in `validate_arch`, `kernel_target_triple`,
   `kernel_target_json`, `boot_target_triple`, and `boot_efi_filename`
   in `scripts/env.sh`
3. Add a QEMU launch case in `run.sh`
4. Add a linker script in `kernel/linker/`
5. Implement the arch traits in `kernel/src/arch/<arch>/`
