// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/protocol/src/lib.rs

//! Boot protocol types shared between the bootloader and kernel.
//!
//! Defines the [`BootInfo`] structure and associated types that form the
//! contract between the bootloader and the kernel entry point. See
//! `docs/boot-protocol.md` for the full specification.
//!
//! All types are `#[repr(C)]` with stable layout. The [`BOOT_PROTOCOL_VERSION`]
//! constant must match between the bootloader and kernel; the kernel halts at
//! entry if the versions differ.

#![no_std]

/// Current boot protocol version. Increment when `BootInfo` layout or the
/// CPU entry contract changes in a non-backwards-compatible way.
pub const BOOT_PROTOCOL_VERSION: u32 = 2;

// ── Memory map ───────────────────────────────────────────────────────────────

/// Classification of a physical memory region.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryType
{
    /// Available for use by the kernel.
    Usable = 0,
    /// In use by the kernel image or boot modules.
    Loaded = 1,
    /// Reserved by firmware or hardware; must not be used.
    Reserved = 2,
    /// ACPI reclaimable after userspace firmware parsing (devmgr) is complete.
    AcpiReclaimable = 3,
    /// Persistent memory (NVDIMM or similar).
    Persistent = 4,
}

/// A single entry in the physical memory map.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MemoryMapEntry
{
    /// Physical base address of the region.
    pub physical_base: u64,
    /// Size of the region in bytes.
    pub size: u64,
    /// Classification of the region.
    pub memory_type: MemoryType,
}

/// A slice of [`MemoryMapEntry`] values, passed by physical address.
///
/// Entries are sorted by `physical_base` in ascending order and do not overlap.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MemoryMapSlice
{
    /// Physical address of the first entry. Null if `count` is zero.
    pub entries: *const MemoryMapEntry,
    /// Number of entries.
    pub count: u64,
}

// SAFETY: MemoryMapSlice contains raw pointers to boot-time physical memory.
// The bootloader guarantees these pointers are valid until the kernel explicitly
// reclaims the regions. Sharing across threads is safe because the boot sequence
// is single-threaded; the kernel reads the map before SMP is active.
unsafe impl Send for MemoryMapSlice {}
// SAFETY: Same rationale as Send; the map is read-only after population.
unsafe impl Sync for MemoryMapSlice {}

// ── Boot modules ─────────────────────────────────────────────────────────────

/// A boot module loaded by the bootloader (e.g., the init binary).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BootModule
{
    /// Physical base address of the module data.
    pub physical_base: u64,
    /// Size of the module data in bytes (file size, not page-rounded size).
    pub size: u64,
}

/// A slice of [`BootModule`] values, passed by physical address.
///
/// `entries[0]` is always the init binary.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ModuleSlice
{
    /// Physical address of the first entry. Null if `count` is zero.
    pub entries: *const BootModule,
    /// Number of entries.
    pub count: u64,
}

// SAFETY: Same rationale as MemoryMapSlice.
unsafe impl Send for ModuleSlice {}
// SAFETY: Same rationale as MemoryMapSlice.
unsafe impl Sync for ModuleSlice {}

// ── Framebuffer ──────────────────────────────────────────────────────────────

/// Pixel format of the framebuffer.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat
{
    /// Red–Green–Blue–Padding, 8 bits per channel.
    Rgbx8 = 0,
    /// Blue–Green–Red–Padding, 8 bits per channel.
    Bgrx8 = 1,
}

/// Framebuffer description provided by the bootloader.
///
/// When `physical_base` is zero, no framebuffer is available. The kernel and
/// early drivers must handle this case gracefully.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FramebufferInfo
{
    /// Physical base address of the framebuffer. Zero if no framebuffer.
    pub physical_base: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes per row (may exceed `width × bytes_per_pixel`).
    pub stride: u32,
    /// Pixel format.
    pub pixel_format: PixelFormat,
}

impl FramebufferInfo
{
    /// Return a zeroed `FramebufferInfo` indicating no framebuffer is present.
    pub const fn empty() -> Self
    {
        Self {
            physical_base: 0,
            width: 0,
            height: 0,
            stride: 0,
            pixel_format: PixelFormat::Rgbx8,
        }
    }
}

// ── Platform resources ───────────────────────────────────────────────────────

/// Discriminant identifying the kind of platform resource.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceType
{
    /// A memory-mapped I/O region.
    ///
    /// `base`+`size`: physical address range.
    /// `flags` bit 0: 0 = device (uncacheable), 1 = write-combine.
    /// `id`: opaque platform identifier (e.g. ACPI UID); zero if unknown.
    MmioRange = 0,

    /// A hardware interrupt line.
    ///
    /// `base`: unused (zero). `size`: unused (zero).
    /// `id`: interrupt number (GSI on x86-64, PLIC source on RISC-V).
    /// `flags` bit 0: 0 = level, 1 = edge triggered.
    /// `flags` bit 1: 0 = active-high, 1 = active-low.
    IrqLine = 1,

    /// A PCI Express ECAM window.
    ///
    /// `base`+`size`: physical ECAM MMIO range.
    /// `flags`: encoded bus range — bits 7:0 = start bus, bits 15:8 = end bus.
    /// `id`: segment group number (usually zero).
    PciEcam = 2,

    /// A firmware table region passed through to userspace as read-only.
    ///
    /// `base`+`size`: physical address range of the table.
    /// `flags`: reserved, zero.
    /// `id`: opaque identifier for the table type (platform-defined).
    PlatformTable = 3,

    /// An x86 I/O port range (x86-64 only).
    ///
    /// `base`: first port number. `size`: number of consecutive ports.
    /// `flags`: reserved, zero. `id`: opaque platform identifier; zero if unknown.
    IoPortRange = 4,

    /// An IOMMU unit's register range and scope.
    ///
    /// `base`+`size`: physical MMIO range of the IOMMU's registers.
    /// `flags`: reserved for future scope encoding; zero in protocol version 2.
    /// `id`: opaque platform identifier (e.g. ACPI DMAR unit index).
    IommuUnit = 5,
}

/// A structured descriptor for a platform hardware resource.
///
/// The array of `PlatformResource` entries in [`BootInfo`] is sorted by
/// `(resource_type, base)` in ascending order. Within a type, entries do not
/// overlap where overlap is nonsensical.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformResource
{
    /// Discriminant identifying the kind of resource.
    pub resource_type: ResourceType,
    /// Type-specific flags (see [`ResourceType`] documentation).
    pub flags: u32,
    /// Physical base address of the resource (zero for [`ResourceType::IrqLine`]).
    pub base: u64,
    /// Size of the resource in bytes (zero for [`ResourceType::IrqLine`]).
    pub size: u64,
    /// Opaque, type-specific identifier. Do not compare across resource types.
    pub id: u64,
}

/// A slice of [`PlatformResource`] values, passed by physical address.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformResourceSlice
{
    /// Physical address of the first entry. Null if `count` is zero.
    pub entries: *const PlatformResource,
    /// Number of entries.
    pub count: u64,
}

// SAFETY: Same rationale as MemoryMapSlice.
unsafe impl Send for PlatformResourceSlice {}
// SAFETY: Same rationale as MemoryMapSlice.
unsafe impl Sync for PlatformResourceSlice {}

// ── BootInfo ─────────────────────────────────────────────────────────────────

/// Boot information structure populated by the bootloader and passed to the
/// kernel entry point.
///
/// All pointer and address fields hold **physical addresses**. The kernel
/// converts them via its direct physical map once paging is fully established.
///
/// The `version` field must equal [`BOOT_PROTOCOL_VERSION`]; the kernel halts
/// if it does not.
#[repr(C)]
#[derive(Debug)]
pub struct BootInfo
{
    /// Protocol version. Must equal [`BOOT_PROTOCOL_VERSION`].
    pub version: u32,

    /// Physical memory map describing all address ranges.
    pub memory_map: MemoryMapSlice,

    /// Physical base address of the loaded kernel image.
    pub kernel_physical_base: u64,
    /// ELF virtual base address of the kernel image.
    pub kernel_virtual_base: u64,
    /// Total span of the kernel ELF LOAD segments in bytes.
    pub kernel_size: u64,

    /// Boot modules. `modules.entries[0]` is always the init binary.
    pub modules: ModuleSlice,

    /// Framebuffer, if available. `physical_base == 0` means no framebuffer.
    pub framebuffer: FramebufferInfo,

    /// Physical address of the ACPI RSDP (x86-64). Zero on RISC-V or if absent.
    ///
    /// Passed through for userspace consumption (`devmgr`). The kernel does
    /// not parse ACPI tables.
    pub acpi_rsdp: u64,

    /// Physical address of the Device Tree blob (RISC-V). Zero on x86-64 or if absent.
    ///
    /// Passed through for userspace consumption (`devmgr`). The kernel does
    /// not parse the Device Tree.
    pub device_tree: u64,

    /// Structured platform resource descriptors extracted from firmware tables.
    ///
    /// The kernel mints initial capabilities from these entries. Sorted by
    /// `(resource_type, base)` in ascending order.
    pub platform_resources: PlatformResourceSlice,

    /// Physical address of a null-terminated kernel command line string.
    ///
    /// May point to a single null byte if no command line was specified.
    pub command_line: *const u8,
    /// Length of the command line string in bytes, excluding the null terminator.
    pub command_line_len: u64,
}
