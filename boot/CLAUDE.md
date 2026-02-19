# Bootloader — AI Context

@README.md
@../docs/coding-standards.md

## What the Bootloader Does

1. Run as a UEFI application under firmware
2. Open the kernel ELF and boot module files from disk
3. Allocate physical memory for all loaded images via UEFI memory services
4. Query the UEFI memory map
5. Parse firmware tables (ACPI on x86-64, Device Tree on RISC-V) into structured
   `PlatformResource` entries
6. Set up initial page tables mapping the kernel at its ELF virtual addresses with
   correct permissions; establish the kernel stack
7. Call `ExitBootServices` (UEFI firmware is unavailable after this point)
8. Populate the `BootInfo` structure in memory
9. Jump to `kernel_entry(boot_info: *const BootInfo) -> !`

## Critical Invariants

1. **`ExitBootServices` before the jump.** UEFI boot services must not be active at
   kernel entry. No UEFI calls are valid after step 7.

2. **W^X in page tables.** No region may be mapped as both writable and executable.
   ELF segment permissions must be honoured: `.text` = RX, `.rodata` = R, `.data`/
   `.bss` = RW. A mapping error here is invisible until the kernel enables protection
   features and silently corrupts execution.

3. **`BootInfo` pointers hold physical addresses.** All pointer/address fields in
   `BootInfo` are physical addresses. The kernel converts them via the direct map after
   its own page tables are established. Do not store virtual addresses in `BootInfo`.

4. **Protocol version must match.** The `version` field must equal the current protocol
   version (currently **2**). The kernel halts on mismatch. Any change to `BootInfo`
   layout or the entry contract requires a version bump.

5. **First boot module is always `init`.** `modules.entries[0]` is the init binary.
   The kernel depends on this ordering.

6. **Memory map is sorted and non-overlapping.** Entries sorted by `physical_base`
   ascending; no two entries overlap.

7. **`PlatformResource` entries are sorted by `(resource_type, base)`.** Within a
   type, entries do not overlap where overlap is nonsensical.

## CPU State at Kernel Entry

The bootloader is responsible for establishing this state before jumping.

### x86-64

| Item | Required state |
|---|---|
| Mode | 64-bit long mode |
| Interrupts | Disabled (`IF = 0`) |
| Direction flag | Clear (`DF = 0`) |
| Paging | Enabled; kernel mapped at ELF virtual addresses |
| Stack | Valid, ≥ 64 KiB; `rsp` aligned to 16 bytes |
| Argument | `rdi` = physical address of `BootInfo` |
| FPU/SSE/AVX | **Not initialised** |
| GDT | Bootloader-provided (kernel replaces it in Phase 5) |
| IDT | **Not loaded** (kernel installs its own in Phase 5) |

### RISC-V (RV64GC)

| Item | Required state |
|---|---|
| Privilege | Supervisor mode (S-mode) |
| Interrupts | Disabled (`SIE = 0`) |
| MMU | Sv48 enabled; kernel mapped at ELF virtual addresses |
| Stack | Valid, ≥ 64 KiB |
| Arguments | `a0` = physical address of `BootInfo`, `a1` = boot hart ID |
| FPU | **Not initialised** |
| Secondary harts | Held in a firmware spin loop; kernel releases them in Phase 10 |

## Firmware Table Parsing

The bootloader parses ACPI (x86-64) or Device Tree (RISC-V) and produces structured
`PlatformResource` entries. This keeps firmware parsing complexity out of the kernel's
trusted computing base.

The raw firmware table pointers (ACPI RSDP or Device Tree blob) are also passed through
in `BootInfo` as read-only frame addresses for `devmgr` to parse in full.

**`PlatformResource` types and key validation constraints:**

| Type | Key constraints |
|---|---|
| `MmioRange` | Base and size must be page-aligned |
| `IrqLine` | `id` field holds the GSI (x86) or PLIC source number (RISC-V) |
| `PciEcam` | Base must be page-aligned; covers one or more PCI segments |
| `PlatformTable` | Base must be page-aligned (ACPI table or DT node) |
| `IoPortRange` | **x86-64 only**; base ≤ 0xFFFF; base + size ≤ 0x10000 |
| `IommuUnit` | Base must be page-aligned |

Do not emit `IoPortRange` entries on RISC-V.

## Common Pitfalls

- Do not call UEFI boot services after `ExitBootServices`
- Do not map any region as both writable and executable
- Do not forget to zero BSS segments of the kernel ELF before jumping
- Do not forget to identity-map (or otherwise keep accessible) the `BootInfo` region
  and boot module regions — the kernel reads them before building its own page tables
- Do not store virtual addresses in `BootInfo`; all pointers are physical addresses
- The framebuffer may not be present (`physical_base = 0`); handle the absent case
  gracefully
- Do not emit `IoPortRange` `PlatformResource` entries on RISC-V
- A `BootInfo` layout change always requires a protocol version bump
