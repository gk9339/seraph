# shared

Internal utility crates with no cross-boundary stability obligation. See
[abi/README.md](../abi/README.md) for the contract crates.

| Crate | Purpose |
|---|---|
| `elf/` | ELF64 parser — header validation, segment enumeration |
| `font/` | Embedded 9×20 bitmap font for early console output |
| `syscall/` | Userspace syscall wrappers — inline asm over `abi/syscall/` |

---

## Summarized By

None
