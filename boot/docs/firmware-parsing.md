# Firmware Parsing

## Overview

The bootloader performs *shallow* firmware parsing: enough to extract structured
`PlatformResource` entries that the kernel can consume to mint initial capabilities.
Deep interpretation — full ACPI namespace evaluation, PCI enumeration, Device Tree
property resolution — is deferred to `devmgr` in userspace.

On x86-64, the ACPI tables are the source. On RISC-V, the Device Tree blob is the
source. In both cases, the raw table pointer is also recorded in `BootInfo` as an
opaque physical address for userspace to parse fully.

This design keeps firmware parsing complexity out of the kernel's trusted computing
base. Bugs in ACPI or Device Tree parsing can produce incorrect `PlatformResource`
entries, but cannot corrupt the kernel itself — the kernel validates all entries in
Phase 6 before minting capabilities from them.

---

## Architecture Dispatch

Firmware table location is obtained from `EFI_CONFIGURATION_TABLE`:

| Architecture | GUID | `BootInfo` field |
|---|---|---|
| x86-64 | `EFI_ACPI_20_TABLE_GUID` | `acpi_rsdp` |
| RISC-V | `EFI_DTB_TABLE_GUID` | `device_tree` |

The configuration table is a flat array (`SystemTable->NumberOfTableEntries` entries,
each a `(GUID, pointer)` pair). The bootloader scans the array for the relevant GUID
and records the physical address of the pointed-to table in the appropriate `BootInfo`
field. If the GUID is absent, the field is zeroed.

On x86-64, the `device_tree` field is always zero. On RISC-V, the `acpi_rsdp` field
is always zero. Neither field is set on both architectures simultaneously.

---

## ACPI Parsing (x86-64)

### Table Walk

```
RSDP (Root System Description Pointer)
 └── XSDT (Extended System Description Table)
      └── table headers for: MADT, MCFG, DMAR, FADT, and others
```

The bootloader locates the RSDP from the UEFI configuration table. The RSDP's
`XsdtAddress` field gives the physical address of the XSDT (64-bit table pointer;
the RSDT at `RsdtAddress` is not used). The XSDT header is validated (signature
`"XSDT"`, `Revision >= 1`, basic length check). The entry array in the XSDT body
contains 64-bit physical addresses of other ACPI tables.

For each entry in the XSDT, the bootloader reads the first 8 bytes (signature and
length), then reads the full table if the signature matches one of interest. Unknown
signatures are skipped.

### MADT (Multiple APIC Description Table)

The MADT contains interrupt controller entries as a variable-length sequence of
typed records. The bootloader processes these types:

**I/O APIC (type 1):**
```
Produces:  MmioRange
base:      IoApicAddress field (physical base of I/O APIC registers)
size:      0x1000 (I/O APIC register space is one page)
flags:     0 (device, uncacheable)
id:        ApicId field from the record
```

**Interrupt Source Override (type 2):**
```
Produces:  IrqLine
id:        GlobalSystemInterrupt field (the GSI number)
flags:     bit 0 = (IntiFlags.Polarity == 3) ? 1 : 0   (1 = active-low → edge)
           bit 1 = (IntiFlags.TriggerMode == 3) ? 1 : 0 (1 = level → active-low)
```
Source overrides record that a legacy ISA IRQ is routed to a different GSI with
possibly different polarity or trigger mode. The bootloader emits one `IrqLine` entry
per override.

**NMI Source (type 3):** Skipped — NMI handling is a kernel concern, not a driver
concern, and does not produce a `PlatformResource`.

**Local APIC address override (type 5):** Updates the base address of the local APIC
register region; the bootloader records this address for potential use but does not
emit a `PlatformResource` entry (the local APIC is a per-CPU resource managed by the
kernel, not delegated to drivers).

All other MADT entry types are skipped.

### MCFG (PCI Memory-Mapped Configuration)

```
For each MCFG allocation entry:
Produces:  PciEcam
base:      BaseAddress field (physical base of ECAM window)
size:      (EndBusNumber - StartBusNumber + 1) * 256 * 4096
           (each bus has 256 devices, each device has 4 KiB of config space)
flags:     StartBusNumber | (EndBusNumber << 8)   (encoded bus range)
id:        PciSegmentGroupNumber field
```

MCFG allocation entries describe contiguous PCI Express configuration space windows.
One `PciEcam` entry is emitted per MCFG allocation entry.

### DMAR (DMA Remapping Reporting) — VT-d

```
For each Remapping Hardware Unit Definition (RHUD):
Produces:  IommuUnit
base:      RegisterBaseAddress field (physical base of VT-d registers)
size:      0x1000 (IOMMU register space is conventionally one page;
                   actual size from RHUD IncludeAllFlag / segment scope)
flags:     0 (reserved; scope encoding is a future extension)
id:        index of this RHUD in the DMAR table (0-based)
```

DMA remapping hardware units describe the IOMMU units on the platform. One
`IommuUnit` entry is emitted per RHUD.

### FADT and Other Tables

Tables not listed above (FADT, SSDT, BERT, EINJ, and others) are recorded as
`PlatformTable` entries:

```
Produces:  PlatformTable
base:      physical address of the table
size:      table header Length field
flags:     0 (reserved)
id:        0 (opaque; devmgr identifies tables by their signature at this address)
```

The RSDP itself is also recorded as a `PlatformTable` entry (`base` = `acpi_rsdp`,
`size` = `sizeof(RSDP)`) so that devmgr can navigate the full ACPI tree from a
single entry. Similarly, the XSDT is recorded as a `PlatformTable` entry.

### I/O Port Ranges

x86-64 platform devices use I/O port ranges for configuration. The bootloader emits
`IoPortRange` entries for well-known legacy port ranges that drivers may need:

| Device | Port range |
|---|---|
| PCI configuration (legacy) | `0xCF8`–`0xCFF` (8 ports) |
| CMOS / RTC | `0x70`–`0x71` (2 ports) |
| i8042 keyboard / PS/2 | `0x60`, `0x64` (2 ports) |
| Serial COM1 | `0x3F8`–`0x3FF` (8 ports) |
| Serial COM2 | `0x2F8`–`0x2FF` (8 ports) |

Additional port ranges may be sourced from ACPI `_CRS` methods if the ACPI evaluator
is implemented in devmgr (deferred). The bootloader emits only the hard-coded legacy
ranges listed above. Ranges where `base + size > 0x10000` are rejected; `IoPortRange`
entries are never emitted on RISC-V.

---

## Device Tree Parsing (RISC-V)

### DTB Location and Validation

The Device Tree blob is located via `EFI_DTB_TABLE_GUID` in the UEFI configuration
table. The bootloader validates the DTB header:

```
magic == 0xD00DFEED (big-endian, as in the FDT spec)
version >= 17
last_comp_version <= 17
totalsize > sizeof(fdt_header)
```

If validation fails, the `device_tree` field in `BootInfo` is zeroed and firmware
parsing proceeds without a DTB; all `PlatformResource` counts are zero. This is a
fatal condition on RISC-V where the DTB is the only firmware description source.

### FDT Walking

The bootloader walks the FDT in a single pass using the flat device tree structure:

```
FDT structure block contains a sequence of tokens:
  FDT_BEGIN_NODE (0x00000001) — node start, followed by null-terminated name
  FDT_END_NODE   (0x00000002) — node end
  FDT_PROP       (0x00000003) — property: u32 len, u32 nameoff, then data bytes
  FDT_NOP        (0x00000004) — ignored
  FDT_END        (0x00000009) — end of structure block
```

The strings block maps `nameoff` values to property names. The bootloader matches
nodes and properties by name string comparison.

### RISC-V Resource Extraction

**MMIO regions** — nodes with a `reg` property (physical base/size pairs) and a
`compatible` string that suggests a peripheral device emit `MmioRange` entries. The
bootloader uses a conservative matching strategy: nodes with `compatible` strings
containing known peripheral class strings (e.g. `ns16550`, `virtio`, `sifive`)
produce entries. Unknown compatible strings are skipped; devmgr performs full
evaluation.

**Interrupt lines** — nodes with an `interrupts` property and an `interrupt-parent`
that resolves to a PLIC emit `IrqLine` entries. The PLIC source number comes from the
`interrupts` property value. Trigger mode is derived from `interrupt-cells` encoding
or assumed to be level-triggered if not specified.

**PCI host bridge** — nodes with `compatible = "pci-host-ecam-generic"` (or similar)
produce `PciEcam` entries. The bus range comes from `bus-range`, the ECAM base and
size from `reg`.

**IOMMU** — nodes with `compatible` strings matching known RISC-V IOMMU
implementations (e.g. `riscv,iommu`) produce `IommuUnit` entries using their `reg`
property.

**Whole DTB** — the entire DTB is recorded as a single `PlatformTable` entry with
`base = BootInfo.device_tree` and `size = fdt_header.totalsize`. This allows devmgr
to perform its own complete walk.

---

## PlatformResource Array Construction

### Collection

All emitted entries are collected into a temporary fixed-size array during firmware
parsing. The maximum number of entries is a compile-time constant
(`PLATFORM_RESOURCE_MAX`) chosen to accommodate the largest expected platform
configuration with room to spare. Overflow of this limit (more entries than the array
can hold) results in the excess entries being discarded with a warning message; the
entries most likely to be discarded are the least significant ones (later ACPI tables
or obscure Device Tree nodes).

### Sorting

After collection, entries are sorted by `(resource_type, base)` in ascending order.
For `IrqLine` entries, `base` is zero; sorting by `id` (interrupt number) is applied
as a secondary key. The sort is a simple insertion sort or similar O(N²) algorithm —
the entry count is small enough (typically fewer than 200 entries on real hardware)
that algorithm complexity is irrelevant.

### Validation

Before finalising the array, each entry is checked:

```
MmioRange, PciEcam, PlatformTable, IommuUnit:
  base must be page-aligned (PAGE_SIZE = 4096)
  size must be > 0 and page-aligned
  base + size must not overflow u64

IoPortRange:
  base must be ≤ 0xFFFF
  base + size must be ≤ 0x10000
  Never emitted on RISC-V

IrqLine:
  id must be within the platform's GSI range (x86-64: 0..=255;
  RISC-V: 0..=1023 for PLIC; actual limit is platform-specific)
```

Entries failing validation are dropped with a warning message. The validation here
is a best-effort sanity check by the bootloader; the kernel performs its own
independent validation in Phase 6.

### Physical Allocation

The final `PlatformResource` array is copied into a region allocated via
`AllocatePages(AllocateAnyPages, EfiLoaderData, ...)`. The physical address of this
region becomes `BootInfo.platform_resources.entries`. The entry count is stored in
`BootInfo.platform_resources.count`.

---

## Parsing Depth

The bootloader's firmware parsing is deliberately shallow. It does not:

- Evaluate ACPI AML bytecode (DSDT, SSDT method evaluation)
- Walk the ACPI namespace to resolve `_CRS`, `_HID`, or device dependencies
- Fully parse Device Tree `interrupt-map` properties or complex `ranges` translations
- Enumerate PCI buses or read PCI configuration space
- Identify specific device models or driver requirements

All of these are `devmgr`'s responsibility. The bootloader produces the minimum set of
structured descriptors needed for the kernel to mint initial capabilities, and passes
the raw firmware tables through for userspace to interpret.
