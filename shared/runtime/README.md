# shared/runtime

Userspace process runtime: `_start()` entry point and panic handler for all
normal Seraph userspace processes (everything except init and ktest).

Reads the `ProcessInfo` handover struct from the well-known virtual address,
constructs a `StartupInfo`, and calls the binary's `main()` function. If
`main()` returns, calls `sys_thread_exit()` as a safety net.

---

## Source Layout

```
shared/runtime/
├── Cargo.toml                  # Workspace member; no_std library
├── README.md
└── src/
    └── lib.rs                  # _start() entry, panic handler
```

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/architecture.md](../../docs/architecture.md) | System design, process startup flow |
| [abi/process-abi](../../abi/process-abi/README.md) | ProcessInfo, StartupInfo, main() signature |
| [docs/coding-standards.md](../../docs/coding-standards.md) | Formatting, naming, safety rules |

---

## Summarized By

None
