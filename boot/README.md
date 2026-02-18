# boot

UEFI bootloader for Seraph. Loads the kernel ELF and boot modules, establishes
initial page tables, parses platform firmware tables (ACPI/Device Tree) into
structured `PlatformResource` descriptors, and jumps to the kernel entry point.

The boot protocol contract — CPU state at entry, `BootInfo` structure layout, and
`PlatformResource` format — is documented in
[docs/boot-protocol.md](../docs/boot-protocol.md).
