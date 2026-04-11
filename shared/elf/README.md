# shared/elf

ELF64 parser for Seraph userspace components.

`no_std`, no external dependencies. Provides header validation, segment
enumeration, and permission mapping. Does not allocate or perform I/O.

Used by `init` (minimal ELF loader for procmgr) and `procmgr` (loads all other
processes). No stability obligation; internal code reuse only.

---

## Source Layout

```
shared/elf/
├── Cargo.toml                  # Workspace member; no_std library
├── README.md
└── src/
    └── lib.rs                  # ELF64 header validation, segment iteration
```

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/architecture.md](../../docs/architecture.md) | System design, init/procmgr roles |
| [docs/boot-protocol.md](../../docs/boot-protocol.md) | Boot module format |
| [docs/coding-standards.md](../../docs/coding-standards.md) | Formatting, naming, safety rules |

---

## Summarized By

None
