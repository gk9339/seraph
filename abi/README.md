# abi

Binary contract crates that cross component or privilege boundaries. Changes are
ABI breaks. See [shared/README.md](../shared/README.md) for non-contract crates.

| Crate | Purpose |
|---|---|
| `boot-protocol/` | `BootInfo` and associated types — boot ABI between bootloader and kernel |
| `init-protocol/` | `InitInfo` and associated types — kernel-to-init handover contract |
| `process-abi/` | `ProcessInfo`, `StartupInfo`, and `main()` convention — process startup ABI |
| `syscall/` | Syscall numbers, error codes, and constants — ABI between kernel and userspace |

---

## Summarized By

None
