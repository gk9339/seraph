# UEFI Environment

## Overview

The bootloader is a UEFI application. Before `ExitBootServices`, all hardware access
and memory allocation goes through UEFI firmware services. After `ExitBootServices`,
UEFI boot services are permanently unavailable; the bootloader operates exclusively
from pre-allocated memory.

This document describes the UEFI protocols the bootloader uses, the allocation
strategy, how the memory map is acquired, the `ExitBootServices` call, and the error
handling strategy throughout.

---

## UEFI Protocol Usage

| Protocol | Handle method | Purpose | Required |
|---|---|---|---|
| `EFI_LOADED_IMAGE_PROTOCOL` | `HandleProtocol(image_handle)` | Obtain device handle for the boot volume | Yes |
| `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` | `HandleProtocol(device_handle)` | Open the ESP root directory | Yes |
| `EFI_GRAPHICS_OUTPUT_PROTOCOL` | `LocateProtocol(NULL)` | Record framebuffer address, dimensions, format | No |
| `EFI_GET_MEMORY_MAP` | `BootServices->GetMemoryMap` | Query physical memory layout | Yes |
| `EFI_ALLOCATE_PAGES` | `BootServices->AllocatePages` | Allocate physical memory for all loaded data | Yes |
| `EFI_CONFIGURATION_TABLE` | `SystemTable->ConfigurationTable` | Locate ACPI RSDP or Device Tree blob | Arch-specific |

`EFI_GRAPHICS_OUTPUT_PROTOCOL` is optional — its absence is handled gracefully by
zeroing the `framebuffer.physical_base` field in `BootInfo`. A headless system or a
virtual machine without a GOP framebuffer is a valid configuration.

`EFI_CONFIGURATION_TABLE` entries are needed for firmware table parsing: on x86-64
the ACPI `EFI_ACPI_20_TABLE_GUID` entry locates the RSDP; on RISC-V the
`EFI_DTB_TABLE_GUID` entry locates the Device Tree blob.

---

## ESP Volume Discovery

The EFI System Partition is accessed as follows:

```
1. image_handle → EFI_LOADED_IMAGE_PROTOCOL → DeviceHandle
2. DeviceHandle → EFI_SIMPLE_FILE_SYSTEM_PROTOCOL
3. SimpleFileSystem->OpenVolume() → root EFI_FILE_PROTOCOL handle
4. root->Open("\EFI\seraph\seraph-kernel") → kernel file handle
5. root->Open("\EFI\seraph\init") → init module file handle
```

The kernel and init binary are opened as read-only files. Their sizes are determined
via `EFI_FILE_INFO` before reading. The files are read into physical memory allocated
by `AllocatePages` at the addresses required by their ELF headers; see
[elf-loading.md](elf-loading.md) for how segment placement works.

---

## Memory Allocation Strategy

All allocation before `ExitBootServices` goes through `AllocatePages`. Two allocation
modes are used:

**`AllocateAnyPages`** — used for data whose physical address does not matter:
`BootInfo` structure, `PlatformResource` array, memory map buffer, `MemoryMapEntry`
array. The firmware selects a free physical page range.

**`AllocateAddress`** — used for ELF LOAD segments that specify a physical address
in their `p_paddr` field, and for page table frames when a specific alignment is
needed. If the requested address range is already in use, allocation fails fatally.

All allocation uses memory type `EfiLoaderData`. UEFI memory map entries for
`EfiLoaderData` regions translate to `MemoryKind::Loaded` in the boot protocol,
signalling to the kernel that these regions are in use and must not be reused until
explicitly reclaimed.

There is no deallocation path before `ExitBootServices`. Memory is allocated once
and used; the bootloader does not implement a heap. UEFI boot services terminate
before any reclamation would be relevant.

---

## Memory Map Acquisition

The UEFI memory map must be queried as the last action before `ExitBootServices`.
Every call to `AllocatePages` (or any other `BootServices` function that allocates
memory) invalidates the previous map key. Querying the map early and then allocating
more memory produces a stale key that causes `ExitBootServices` to fail.

The acquisition sequence:

```
1. Call GetMemoryMap(0, NULL, &map_key, &desc_size, &desc_version)
   to obtain the required buffer size.
2. AllocatePages(AllocateAnyPages, EfiLoaderData, pages_needed, &buf_addr)
   Note: this allocation itself invalidates any prior map key.
3. Call GetMemoryMap(buf_size, buf_addr, &map_key, &desc_size, &desc_version)
   to fill the buffer. The map_key from this call is the correct one to use.
4. Translate entries: UEFI memory types → MemoryKind (see translation table below).
5. Sort entries by physical_base ascending.
```

The buffer allocation in step 2 increases the map size by one entry (the new
`EfiLoaderData` region). The buffer allocated in step 2 must be sized to accommodate
this extra entry; in practice, adding 16 extra entries of slack when sizing the buffer
is sufficient.

### UEFI Memory Type Translation

| UEFI `EFI_MEMORY_TYPE` | `MemoryKind` |
|---|---|
| `EfiConventionalMemory` | `Usable` |
| `EfiLoaderCode`, `EfiLoaderData` | `Loaded` |
| `EfiBootServicesCode`, `EfiBootServicesData` | `Usable` (reclaimable after UEFI exit) |
| `EfiRuntimeServicesCode`, `EfiRuntimeServicesData` | `Reserved` |
| `EfiACPIReclaimMemory` | `AcpiReclaimable` |
| `EfiACPIMemoryNVS` | `Reserved` |
| `EfiMemoryMappedIO`, `EfiMemoryMappedIOPortSpace` | `Reserved` |
| `EfiPersistentMemory` | `Persistent` |
| All other types | `Reserved` |

`EfiBootServicesCode` and `EfiBootServicesData` are translated to `Usable` because
those regions are no longer in use once UEFI boot services have exited. The kernel
may reclaim them. `EfiRuntimeServicesCode` and `EfiRuntimeServicesData` are marked
`Reserved` because Seraph does not use UEFI runtime services; those regions are
treated as off-limits rather than reclaimed.

---

## ExitBootServices

The call sequence:

```
1. Perform the final memory map acquisition (above).
2. Call BootServices->ExitBootServices(image_handle, map_key).
3. If the call returns EFI_INVALID_PARAMETER (stale map key):
   a. Call GetMemoryMap again with the existing buffer (no new allocation).
   b. Update map_key from the new call.
   c. Retry ExitBootServices once.
   d. If still failing: halt immediately — the environment is unrecoverable.
4. On success: UEFI boot services are now permanently unavailable.
```

The retry handles the rare case where UEFI performs an internal allocation between
step 1 and step 2 — some firmware implementations do this for housekeeping. The
retry uses the existing buffer rather than allocating a new one (which would
invalidate the key again).

### Post-Exit Constraints

After `ExitBootServices` returns:

- `BootServices` pointer is invalid; calling any boot service causes undefined behaviour
- `RuntimeServices` pointer remains technically valid (UEFI runtime services), but
  Seraph does not use runtime services and makes no runtime calls
- Memory descriptors in the acquired map buffer remain valid; the buffer was allocated
  as `EfiLoaderData` and is not reclaimed by UEFI
- Only pre-allocated memory (from `AllocatePages` calls made before the exit) is
  available; no further allocation is possible

The bootloader performs no allocation-dependent operations after `ExitBootServices`.
`BootInfo` population and kernel handoff use only data gathered before the exit.

---

## Error Handling Strategy

All errors in the bootloader are fatal. There is no recovery path, no retry beyond
the single `ExitBootServices` retry described above, and no fallback configuration.
A bootloader that cannot complete its sequence cannot safely hand off to the kernel.

### BootError Type

```rust
#[derive(Debug)]
pub enum BootError
{
    /// A required UEFI protocol was not found.
    ProtocolNotFound(&'static str),

    /// A UEFI call returned an unexpected status code.
    UefiError(usize),

    /// A required file was not found on the ESP.
    FileNotFound(&'static str),

    /// The kernel ELF failed validation.
    InvalidElf(&'static str),

    /// An ELF segment has both writable and executable permissions (W^X violation).
    WxViolation,

    /// A physical memory allocation failed.
    OutOfMemory,

    /// ExitBootServices failed after retry.
    ExitBootServicesFailed,
}
```

All fallible functions in the bootloader return `Result<T, BootError>`. The top-level
`efi_main` function propagates errors to a single fatal handler that reports the error
and halts.

### Error Reporting

Before `ExitBootServices`, error messages are written to the UEFI console output
(`SystemTable->ConOut`) and, if a GOP framebuffer is available, to the framebuffer
using a minimal pixel-writing text renderer. After `ExitBootServices`, only the
framebuffer path is available (UEFI console is no longer accessible).

Error reporting writes a short descriptive message and halts:

```
SERAPH BOOT FATAL: <error description>
```

No elaborate formatting, no stack traces, no recovery prompts. The message is
sufficient to identify which step failed and why.

---

## Console Output

### Before ExitBootServices

Two output paths are available:

**UEFI console** — `SystemTable->ConOut->OutputString`. This is always available but
the display quality depends on the firmware. It supports wide characters (`CHAR16`);
the bootloader converts ASCII boot messages to wide character strings before output.

**GOP framebuffer** — if `EFI_GRAPHICS_OUTPUT_PROTOCOL` is available and reports a
pixel framebuffer, the bootloader maps a minimal bitmap font and writes directly to
the framebuffer. This is more reliable on modern hardware where the UEFI console
implementation may be slow or redirect to a serial port.

### After ExitBootServices

Only the GOP framebuffer path is available. The framebuffer's physical address and
dimensions were recorded during step 1. The bootloader writes any post-exit messages
by computing pixel offsets from the physical base address, which remains directly
accessible (the identity map from step 5 covers the framebuffer region or the
physical address is used directly if paging is not yet switched).

In practice the bootloader has no output after `ExitBootServices` beyond an error
message in the failure case. The normal path proceeds directly to `BootInfo`
population and kernel handoff.
