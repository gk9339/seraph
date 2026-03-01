# abi

ABI-defining crates. These crates define the binary interfaces that cross
component or privilege boundaries and carry stability guarantees.

| Crate | Purpose |
|---|---|
| `boot-protocol/` | `BootInfo` structure and associated types — boot ABI between bootloader and kernel |
| `syscall/` | Syscall numbers, argument layout, return codes, calling convention constants |

## Distinction from `shared/`

`abi/` crates define contracts: the kernel and its callers must agree on these
definitions at the binary level. `shared/` crates are internal code reuse with
no cross-boundary stability obligation.
