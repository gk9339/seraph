# abi/boot-protocol

Binary boot protocol contract between the bootloader and the kernel.

Defines `BootInfo` and all associated types passed from the bootloader to the
kernel entry point. Includes the `BOOT_PROTOCOL_VERSION` constant; the kernel
halts at entry if the bootloader's version does not match.

**Constraints:** `no_std`, `#[repr(C)]` for all types, no dependencies outside
`core`. Changes that alter `BootInfo` layout or the CPU entry contract MUST
increment `BOOT_PROTOCOL_VERSION`.

See [docs/boot-protocol.md](../../docs/boot-protocol.md) for the full
specification — field semantics, memory ownership rules, and entry requirements.

---

## Summarized By

None
