# Boot Protocol

## Overview

Seraph uses a custom UEFI-native boot protocol. The bootloader is a UEFI application
that runs under the system firmware, loads the kernel and initial modules, establishes
a known CPU and memory state, and jumps to the kernel entry point. The kernel makes no
assumptions about the environment beyond what this document guarantees.

The protocol is defined here. The bootloader in `boot/` is the reference implementation.
Any compliant bootloader that satisfies this contract may be used in its place.

---

## Boot Flow

1. UEFI firmware loads the bootloader from the EFI System Partition
2. Bootloader locates and reads the kernel ELF and boot modules from disk
3. Bootloader allocates physical memory for all loaded images
4. Bootloader queries the UEFI memory map
5. Bootloader sets up initial page tables mapping the kernel at its virtual addresses
6. Bootloader calls `ExitBootServices` — firmware services are no longer available
7. Bootloader populates the boot information structure
8. Bootloader jumps to the kernel entry point

After step 6, the system is under full bootloader and then kernel control. UEFI
runtime services are not used; the firmware is considered done.

---

## Kernel Entry Point

The kernel must export a single entry point symbol. The bootloader jumps to this
address after establishing the CPU state described below.

**Signature:**

```rust
#[no_mangle]
pub extern "C" fn kernel_entry(boot_info: *const BootInfo) -> ! {
    ...
}
```

The entry point receives a single argument: a pointer to the boot information
structure (see below). The pointer is valid and the structure is fully populated
before the jump occurs.

The entry point must not return. The bootloader does not provide a return address
in any meaningful context.

---

## CPU State at Entry

The following state is guaranteed by the bootloader on both architectures.
The kernel must not assume anything beyond what is listed here.

### x86-64

| Item | Guaranteed state |
|---|---|
| Mode | 64-bit long mode |
| Interrupts | Disabled (`IF` = 0) |
| Direction flag | Clear (`DF` = 0) |
| Paging | Enabled; kernel mapped at intended virtual addresses |
| Stack | Valid; at least 64 KiB available |
| `rdi` | Physical address of `BootInfo` structure |
| Floating point | Not initialised; kernel must not use SSE/AVX before enabling |
| GDT | Bootloader-provided; kernel replaces it during early initialisation |
| IDT | Not loaded; interrupts must remain disabled until the kernel installs its own |

### RISC-V (RV64GC)

| Item | Guaranteed state |
|---|---|
| Privilege level | Supervisor mode |
| Interrupts | Disabled (`sstatus.SIE` = 0) |
| MMU | Enabled (Sv48); kernel mapped at intended virtual addresses |
| Stack | Valid; at least 64 KiB available |
| `a0` | Physical address of `BootInfo` structure |
| `a1` | Hart ID of the booting hart |
| Floating point | Not initialised |

On RISC-V, secondary harts are held in a spin loop by the bootloader and are
released by the kernel when it is ready to bring them up.

---

## Page Table State at Entry

The bootloader establishes a minimal page table sufficient for the kernel to begin
executing at its intended virtual addresses. This table includes:

- The kernel image mapped at its ELF-specified virtual addresses (text, rodata,
  data, bss), with permissions matching each segment (W^X enforced)
- An identity map of the physical memory region containing the `BootInfo` structure
  and all boot modules, so the kernel can read them before establishing its own
  mappings
- The bootloader's own stack, mapped at the address in use at entry

The kernel is expected to replace this table with its own during early
initialisation. The bootloader's page table is not intended to be permanent.

---

## Boot Information Structure

The `BootInfo` structure is allocated in physical memory by the bootloader and
remains valid until the kernel explicitly reclaims or unmaps the region. All
pointer fields within the structure refer to physical addresses unless otherwise
noted — the kernel converts these using its direct physical map once paging is
fully established.

```rust
#[repr(C)]
pub struct BootInfo {
    /// Protocol version. Must match the kernel's expected version.
    pub version: u32,

    /// Physical memory map describing all address ranges.
    pub memory_map: MemoryMapSlice,

    /// Physical and virtual base addresses of the loaded kernel image.
    pub kernel_physical_base: u64,
    pub kernel_virtual_base: u64,
    pub kernel_size: u64,

    /// Boot modules (e.g. init binary). First entry is always init.
    pub modules: ModuleSlice,

    /// Framebuffer, if available. Used for early debug output.
    pub framebuffer: FramebufferInfo,

    /// ACPI RSDP physical address (x86-64). Zero if not present.
    pub acpi_rsdp: u64,

    /// Device tree blob physical address (RISC-V). Zero if not present.
    pub device_tree: u64,

    /// Null-terminated kernel command line string. May be empty.
    pub command_line: *const u8,
    pub command_line_len: u64,
}
```

### Memory Map

```rust
#[repr(C)]
pub struct MemoryMapSlice {
    pub entries: *const MemoryMapEntry,
    pub count: u64,
}

#[repr(C)]
pub struct MemoryMapEntry {
    pub physical_base: u64,
    pub size: u64,
    pub kind: MemoryKind,
}

#[repr(u32)]
pub enum MemoryKind {
    /// Available for use by the kernel.
    Usable = 0,
    /// In use by the kernel image or boot modules.
    Loaded = 1,
    /// Reserved by firmware or hardware; must not be used.
    Reserved = 2,
    /// ACPI reclaimable after the kernel has read ACPI tables.
    AcpiReclaimable = 3,
    /// Persistent memory (NVDIMM or similar).
    Persistent = 4,
}
```

The memory map is sorted by `physical_base` in ascending order. Entries do not
overlap. The kernel must not write to `Reserved` regions. `Loaded` regions
containing boot modules may be reclaimed once the kernel has consumed them.

### Boot Modules

```rust
#[repr(C)]
pub struct ModuleSlice {
    pub entries: *const BootModule,
    pub count: u64,
}

#[repr(C)]
pub struct BootModule {
    pub physical_base: u64,
    pub size: u64,
}
```

The first module (`modules.entries[0]`) is always the init binary, as an ELF
executable. Additional modules may follow; their purpose is defined by convention
between the bootloader configuration and the kernel. The kernel must verify the
ELF headers of each module before use.

### Framebuffer

```rust
#[repr(C)]
pub struct FramebufferInfo {
    /// Physical base address of the framebuffer. Zero if no framebuffer.
    pub physical_base: u64,
    pub width: u32,
    pub height: u32,
    /// Bytes per row (may be larger than width × bytes_per_pixel).
    pub stride: u32,
    pub pixel_format: PixelFormat,
}

#[repr(u32)]
pub enum PixelFormat {
    Rgbx8 = 0,
    Bgrx8 = 1,
}
```

The framebuffer is provided on a best-effort basis. The kernel must handle the case
where `physical_base` is zero and no framebuffer is available. This is the expected
state in headless or virtual environments without a display.

---

## Protocol Version

The `version` field in `BootInfo` must be checked by the kernel on entry. If the
version does not match the expected value, the kernel must halt rather than proceed
with a potentially incompatible structure. This prevents silent corruption from
a mismatched bootloader.

The current protocol version is `1`. This value is incremented whenever the
`BootInfo` structure or CPU entry contract changes in a non-backwards-compatible way.

---

## Bootloader Responsibilities

The bootloader must, before jumping to the kernel entry point:

- Verify that the kernel ELF is valid and has a recognisable entry point
- Load all ELF LOAD segments into allocated physical memory
- Zero BSS segments
- Respect ELF segment permissions (readable, writable, executable) when establishing
  page table entries
- Obtain the final UEFI memory map after all allocations are complete
- Call `ExitBootServices` successfully before jumping to the kernel
- Guarantee that the `BootInfo` structure and all referenced data remain mapped
  and readable at kernel entry

The bootloader must not:

- Leave UEFI boot services active at kernel entry
- Map any region as both writable and executable
- Assume anything about the kernel's internal layout beyond the ELF headers
