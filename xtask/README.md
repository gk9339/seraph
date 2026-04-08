# xtask

Build task runner for Seraph. Invoke via `cargo xtask <command>`.

---

## Commands

### `cargo xtask build`

Build Seraph components and populate `sysroot/`.

```
cargo xtask build [--arch x86_64|riscv64] [--release] [--component boot|kernel|init|all]
```

| Option | Default | Description |
|---|---|---|
| `--arch` | `x86_64` | Target architecture |
| `--release` | off | Build in release mode |
| `--component` | `all` | Build a single component (`boot`, `kernel`, `init`, or `all`) |

The sysroot is architecture-specific. Building for a different arch than the
existing sysroot is an error — run `cargo xtask clean` first.

---

### `cargo xtask run`

Build all components (incremental) then launch Seraph under QEMU.

```
cargo xtask run [--arch x86_64|riscv64] [--release] [--gdb] [--headless] [--verbose]
```

| Option | Description |
|---|---|
| `--arch` | Target architecture (default: `x86_64`) |
| `--release` | Use the release build |
| `--gdb` | Start QEMU with a GDB server on localhost:1234; QEMU pauses at startup |
| `--headless` | Run without a display window (`-display none`) |
| `--verbose` | Show all serial output; by default output is filtered until `seraph-boot` appears |

**x86-64** uses KVM acceleration (TCG when `--gdb` is set). Requires OVMF
firmware (`dnf install edk2-ovmf` / `apt install ovmf`).

**RISC-V** uses TCG emulation with edk2 UEFI firmware and OpenSBI (loaded
automatically by QEMU's `virt` machine). Requires edk2 RISC-V firmware
(`dnf install edk2-riscv64` / `apt install qemu-efi-riscv64`).

GDB note: KVM is disabled when `--gdb` is set. TCG is ~5–10× slower than KVM;
expect ~30s to reach the bootloader instead of ~5s. This is required for correct
register visibility — KVM freezes all vCPUs at the reset vector in gdbserver mode.

---

### `cargo xtask clean`

Remove the sysroot (and optionally `target/`).

```
cargo xtask clean [--all]
```

| Option | Description |
|---|---|
| `--all` | Also run `cargo clean` to remove the `target/` directory |

---

### `cargo xtask test`

Run Seraph unit tests on the host target.

```
cargo xtask test [--component boot|protocol|kernel|init|all]
```

Tests compile for the host — no `--arch` flag needed. The workspace-level
`panic=abort` profile does not affect the test harness.

---

## Adding a new command

1. Add a variant to `CliCommand` in `src/cli.rs` with a corresponding `Args` struct.
2. Create `src/commands/<name>.rs` with `pub fn run(ctx: &Context, args: &NameArgs) -> Result<()>`.
3. Add a match arm in `src/main.rs`.
4. Re-export the module in `src/commands/mod.rs`.

---

## Summarized By

None
