# shared/elf

ELF64 parser for Seraph userspace components.

`no_std`, no external dependencies. Provides header validation, segment
enumeration, and permission mapping. Does not allocate or perform I/O.

Used by `init` (minimal ELF loader for procmgr) and `procmgr` (loads all other
processes). No stability obligation; internal code reuse only.

---

## Summarized By

None
