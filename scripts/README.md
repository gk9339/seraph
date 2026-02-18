# scripts

Build system, tooling, and helper scripts. Includes:

- Custom target JSON specifications for `x86_64-seraph-none` and
  `riscv64gc-seraph-none` (used by the kernel and other `no_std` crates)
- Build orchestration scripts for assembling disk images and running under QEMU
- Helper utilities for symbol map generation, linker script selection, and
  cross-compilation environment setup
