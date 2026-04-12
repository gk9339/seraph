// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/dtb.rs

//! Minimal Flattened Device Tree (FDT/DTB) parser.
//!
//! Reads in-place from the DTB blob (identity-mapped by UEFI). No allocation.
//! Assumes `#address-cells = 2` and `#size-cells = 2`, which is standard for
//! RISC-V QEMU virt. All header fields and tokens are big-endian.
//!
//! Error handling: malformed nodes are skipped; partial results are returned.
//! Fatal errors (bad magic, out-of-range offsets) return `None` / zero count.
//!
//! # Extending
//! To add new resource types, add a `for_each_compatible` call in
//! `parse_dtb_resources` with the appropriate compatible string and fill in
//! the `PlatformResource` fields per the boot-protocol spec.

use crate::bprintln;
use boot_protocol::{PlatformResource, ResourceType};

// ── FDT constants ─────────────────────────────────────────────────────────────

const FDT_MAGIC: u32 = 0xd00d_feed;

/// FDT struct block token types (big-endian u32).
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_NOP: u32 = 4;
#[allow(dead_code)] // Used as a sentinel that collapses into the `_ => break` arm.
const FDT_END: u32 = 9;

/// Maximum FDT node nesting depth supported by the walker.
const MAX_DEPTH: usize = 8;

/// Maximum `reg` entries (address+size pairs) extracted per node.
const MAX_REG_ENTRIES: usize = 8;
/// Maximum number of `interrupts` values collected per node.
const MAX_IRQ_ENTRIES: usize = 4;
/// Maximum number of `ranges` entries collected per node.
const MAX_RANGES_ENTRIES: usize = 4;

// ── Public types ──────────────────────────────────────────────────────────────

/// A validated reference to an FDT blob in identity-mapped physical memory.
///
/// Constructed via [`Fdt::from_raw`]. Field offsets are validated against
/// `total_size` at construction time; subsequent reads are bounds-checked.
pub struct Fdt
{
    /// Physical base address of the DTB blob.
    base: u64,
    /// Total blob size in bytes (from header field `totalsize`).
    total_size: u32,
    /// Byte offset of the struct block from blob start.
    off_struct: u32,
    /// Size of the struct block in bytes.
    size_struct: u32,
    /// Byte offset of the strings block from blob start.
    off_strings: u32,
    /// Size of the strings block in bytes.
    size_strings: u32,
}

/// Data extracted from a single FDT node.
#[derive(Clone, Copy)]
pub struct FdtNode
{
    /// MMIO regions: `(base_address, size)` pairs from the `reg` property.
    pub reg_entries: [(u64, u64); MAX_REG_ENTRIES],
    /// Number of valid entries in [`reg_entries`].
    pub reg_count: usize,
    /// Interrupt specifiers from the `interrupts` property (raw u32 values).
    pub irq_entries: [u32; MAX_IRQ_ENTRIES],
    /// Number of valid entries in [`irq_entries`].
    pub irq_count: usize,
    /// PCI-style `ranges` entries: `(child_flags, cpu_addr, size)` tuples.
    /// `child_flags` encodes space type in bits 25:24 (1=I/O, 2=32-bit MMIO,
    /// 3=64-bit MMIO).
    pub ranges_entries: [(u32, u64, u64); MAX_RANGES_ENTRIES],
    /// Number of valid entries in [`ranges_entries`].
    pub ranges_count: usize,
}

// ── Per-depth traversal state (private) ───────────────────────────────────────

/// Property collection state for one open node during tree traversal.
#[derive(Clone, Copy)]
struct NodeState
{
    compatible_matched: bool,
    reg_entries: [(u64, u64); MAX_REG_ENTRIES],
    reg_count: usize,
    irq_entries: [u32; MAX_IRQ_ENTRIES],
    irq_count: usize,
    ranges_entries: [(u32, u64, u64); MAX_RANGES_ENTRIES],
    ranges_count: usize,
}

impl NodeState
{
    const fn new() -> Self
    {
        NodeState {
            compatible_matched: false,
            reg_entries: [(0, 0); MAX_REG_ENTRIES],
            reg_count: 0,
            irq_entries: [0; MAX_IRQ_ENTRIES],
            irq_count: 0,
            ranges_entries: [(0, 0, 0); MAX_RANGES_ENTRIES],
            ranges_count: 0,
        }
    }
}

// ── Fdt implementation ────────────────────────────────────────────────────────

impl Fdt
{
    /// Validate and wrap an FDT blob at the given physical address.
    ///
    /// Returns `None` on bad magic, blob too small, or out-of-range offsets.
    ///
    /// # Safety
    /// `base` must be the physical address of a valid, identity-mapped FDT
    /// blob of at least 40 bytes.
    pub unsafe fn from_raw(base: u64) -> Option<Self>
    {
        // FDT header: 10 big-endian u32 fields (40 bytes total).
        // Field layout (byte offsets):
        //   0: magic  4: totalsize  8: off_dt_struct  12: off_dt_strings
        //  16: off_mem_rsvmap  20: version  24: last_comp_version
        //  28: boot_cpuid_phys  32: size_dt_strings  36: size_dt_struct
        let hdr = base as *const u32;
        // SAFETY: base is identity-mapped by UEFI; reads within firmware-provided DTB blob.
        let magic = u32::from_be(unsafe { core::ptr::read_unaligned(hdr) });
        if magic != FDT_MAGIC
        {
            return None;
        }
        // SAFETY: magic validated; remaining header fields within blob.
        let total_size = u32::from_be(unsafe { core::ptr::read_unaligned(hdr.add(1)) });
        // SAFETY: header fields within validated blob.
        let off_struct = u32::from_be(unsafe { core::ptr::read_unaligned(hdr.add(2)) });
        // SAFETY: header fields within validated blob.
        let off_strings = u32::from_be(unsafe { core::ptr::read_unaligned(hdr.add(3)) });
        // SAFETY: header fields within validated blob.
        let size_strings = u32::from_be(unsafe { core::ptr::read_unaligned(hdr.add(8)) });
        // SAFETY: header fields within validated blob.
        let size_struct = u32::from_be(unsafe { core::ptr::read_unaligned(hdr.add(9)) });

        // Bounds: struct and strings blocks must fit inside total_size.
        if off_struct.checked_add(size_struct)? > total_size
            || off_strings.checked_add(size_strings)? > total_size
        {
            return None;
        }

        Some(Fdt {
            base,
            total_size,
            off_struct,
            size_struct,
            off_strings,
            size_strings,
        })
    }

    /// Total size of the DTB blob in bytes (from the FDT `totalsize` field).
    pub fn total_size(&self) -> u32
    {
        self.total_size
    }

    /// Read a big-endian u32 from the struct block at byte offset `off`.
    /// Returns `None` if the read would exceed the struct block bounds.
    fn read_struct_u32(&self, off: u32) -> Option<u32>
    {
        if off.checked_add(4)? > self.size_struct
        {
            return None;
        }
        let addr = self.base + u64::from(self.off_struct) + u64::from(off);
        // SAFETY: offset validated above; read within struct block bounds.
        Some(u32::from_be(unsafe {
            core::ptr::read_unaligned(addr as *const u32)
        }))
    }

    /// Return a byte slice covering `len` bytes of the struct block at `off`.
    /// Returns an empty slice if out of bounds.
    fn struct_slice(&self, off: u32, len: u32) -> &[u8]
    {
        let end = match off.checked_add(len)
        {
            Some(e) if e <= self.size_struct => e,
            _ => return &[],
        };
        let _ = end;
        let addr = self.base + u64::from(self.off_struct) + u64::from(off);
        // SAFETY: bounds validated above; slice within struct block.
        unsafe { core::slice::from_raw_parts(addr as *const u8, len as usize) }
    }

    /// Return the null-terminated string at `nameoff` in the strings block.
    /// Returns an empty slice if `nameoff` is out of range.
    fn string_at(&self, nameoff: u32) -> &[u8]
    {
        if nameoff >= self.size_strings
        {
            return &[];
        }
        let addr = self.base + u64::from(self.off_strings) + u64::from(nameoff);
        let max_len = (self.size_strings - nameoff) as usize;
        let start = addr as *const u8;
        let mut len = 0;
        // SAFETY: nameoff validated above; pointer arithmetic within strings block.
        while len < max_len && unsafe { *start.add(len) } != 0
        {
            len += 1;
        }
        // SAFETY: len ≤ max_len; slice within strings block.
        unsafe { core::slice::from_raw_parts(start, len) }
    }

    /// Walk the struct block, calling `callback` for every node whose
    /// `compatible` property contains `compat` as one of its strings.
    ///
    /// `callback` returns `true` to continue or `false` to stop early.
    // too_many_lines: DTB traversal state machine; splitting would fragment the
    // node-tracking logic across functions without meaningful abstraction.
    #[allow(clippy::too_many_lines)]
    fn walk_compatible<F>(&self, compat: &[u8], mut callback: F)
    where
        F: FnMut(FdtNode) -> bool,
    {
        // Per-depth node state. Index = depth-1 (depth 0 = before root).
        let mut states = [NodeState::new(); MAX_DEPTH];
        let mut depth: usize = 0;
        let mut off: u32 = 0; // byte offset within struct block

        while let Some(token) = self.read_struct_u32(off)
        {
            off += 4;

            match token
            {
                FDT_BEGIN_NODE =>
                {
                    if depth < MAX_DEPTH
                    {
                        states[depth] = NodeState::new();
                    }
                    depth += 1;
                    // Skip null-terminated, 4-byte-aligned node name.
                    off = skip_node_name(self, off);
                }
                FDT_END_NODE =>
                {
                    if depth == 0
                    {
                        break; // malformed FDT
                    }
                    depth -= 1;
                    if depth < MAX_DEPTH && states[depth].compatible_matched
                    {
                        let s = &states[depth];
                        let node = FdtNode {
                            reg_entries: s.reg_entries,
                            reg_count: s.reg_count,
                            irq_entries: s.irq_entries,
                            irq_count: s.irq_count,
                            ranges_entries: s.ranges_entries,
                            ranges_count: s.ranges_count,
                        };
                        if !callback(node)
                        {
                            break;
                        }
                    }
                }
                FDT_PROP =>
                {
                    let Some(prop_len) = self.read_struct_u32(off)
                    else
                    {
                        break;
                    };
                    off += 4;
                    let Some(nameoff) = self.read_struct_u32(off)
                    else
                    {
                        break;
                    };
                    off += 4;
                    let data_off = off;
                    // Advance past prop data (4-byte aligned).
                    off += (prop_len + 3) & !3;

                    // Only process properties for nodes within stack depth.
                    if depth == 0 || depth > MAX_DEPTH
                    {
                        continue;
                    }
                    let state = &mut states[depth - 1];
                    let name = self.string_at(nameoff);

                    if name == b"compatible"
                    {
                        let data = self.struct_slice(data_off, prop_len);
                        if prop_contains(data, compat)
                        {
                            state.compatible_matched = true;
                        }
                    }
                    else if name == b"reg"
                    {
                        // #address-cells=2, #size-cells=2: each entry = 16 bytes.
                        // Each (address, size) is two big-endian u32 pairs.
                        let data = self.struct_slice(data_off, prop_len);
                        let mut i = 0;
                        while i + 16 <= data.len() && state.reg_count < MAX_REG_ENTRIES
                        {
                            let addr = read_be64(&data[i..]);
                            let size = read_be64(&data[i + 8..]);
                            state.reg_entries[state.reg_count] = (addr, size);
                            state.reg_count += 1;
                            i += 16;
                        }
                    }
                    else if name == b"interrupts"
                    {
                        // #interrupt-cells=1 (typical for PLIC): each entry = 4 bytes.
                        let data = self.struct_slice(data_off, prop_len);
                        let mut i = 0;
                        while i + 4 <= data.len() && state.irq_count < MAX_IRQ_ENTRIES
                        {
                            state.irq_entries[state.irq_count] = read_be32(&data[i..]);
                            state.irq_count += 1;
                            i += 4;
                        }
                    }
                    else if name == b"ranges"
                    {
                        // PCI ranges: (#address-cells=3, #size-cells=2, parent #address-cells=2)
                        // Each entry = 28 bytes: child_hi(u32) child_mid(u32) child_lo(u32)
                        //                        parent_hi(u32) parent_lo(u32)
                        //                        size_hi(u32) size_lo(u32)
                        let data = self.struct_slice(data_off, prop_len);
                        let mut i = 0;
                        while i + 28 <= data.len() && state.ranges_count < MAX_RANGES_ENTRIES
                        {
                            let child_flags = read_be32(&data[i..]);
                            // child_mid:child_lo form the 64-bit child bus address
                            // (ignored for resource extraction — we use parent addr).
                            let parent_addr = read_be64(&data[i + 12..]);
                            let size = read_be64(&data[i + 20..]);
                            state.ranges_entries[state.ranges_count] =
                                (child_flags, parent_addr, size);
                            state.ranges_count += 1;
                            i += 28;
                        }
                    }
                }
                FDT_NOP =>
                {}
                // FDT_END = end of struct block; any other token = malformed/unknown.
                _ => break,
            }
        }
    }

    /// Call `f` for each node whose `compatible` property contains `compat`.
    pub fn for_each_compatible<F: FnMut(&FdtNode)>(&self, compat: &[u8], mut f: F)
    {
        self.walk_compatible(compat, |node| {
            f(&node);
            true
        });
    }

    /// Walk CPU nodes (compatible = "riscv") and call `callback(hart_id)` for
    /// each enabled CPU. CPU nodes use `#address-cells=1, #size-cells=0`, so
    /// `reg` is a single big-endian u32 hart ID.
    ///
    /// `callback` returns `true` to continue or `false` to stop early.
    ///
    /// CPU nodes without a `status` property (or with `status = "okay"`) are
    /// counted as enabled. `status = "disabled"` is skipped.
    // too_many_lines: DTB traversal state machine; splitting would fragment the
    // node-tracking logic across functions without meaningful abstraction.
    #[allow(clippy::too_many_lines)]
    pub fn walk_cpu_nodes<F>(&self, mut callback: F)
    where
        F: FnMut(u32) -> bool,
    {
        // Node state for one open node during traversal.
        #[derive(Clone, Copy)]
        struct CpuNodeState
        {
            is_riscv_cpu: bool,
            reg_u32: u32,
            has_reg: bool,
            disabled: bool,
        }

        let mut states = [CpuNodeState {
            is_riscv_cpu: false,
            reg_u32: 0,
            has_reg: false,
            disabled: false,
        }; MAX_DEPTH];

        let mut depth: usize = 0;
        let mut off: u32 = 0;

        while let Some(token) = self.read_struct_u32(off)
        {
            off += 4;

            match token
            {
                FDT_BEGIN_NODE =>
                {
                    if depth < MAX_DEPTH
                    {
                        states[depth] = CpuNodeState {
                            is_riscv_cpu: false,
                            reg_u32: 0,
                            has_reg: false,
                            disabled: false,
                        };
                    }
                    depth += 1;
                    off = skip_node_name(self, off);
                }
                FDT_END_NODE =>
                {
                    if depth == 0
                    {
                        break;
                    }
                    depth -= 1;
                    if depth < MAX_DEPTH
                    {
                        let s = &states[depth];
                        if s.is_riscv_cpu && s.has_reg && !s.disabled && !callback(s.reg_u32)
                        {
                            break;
                        }
                    }
                }
                FDT_PROP =>
                {
                    let Some(prop_len) = self.read_struct_u32(off)
                    else
                    {
                        break;
                    };
                    off += 4;
                    let Some(nameoff) = self.read_struct_u32(off)
                    else
                    {
                        break;
                    };
                    off += 4;
                    let data_off = off;
                    off += (prop_len + 3) & !3;

                    if depth == 0 || depth > MAX_DEPTH
                    {
                        continue;
                    }
                    let state = &mut states[depth - 1];
                    let name = self.string_at(nameoff);

                    if name == b"compatible"
                    {
                        let data = self.struct_slice(data_off, prop_len);
                        // "riscv" appears as a standalone compatible string in CPU nodes,
                        // or as a prefix in strings like "riscv,sv48". Match either.
                        if prop_contains(data, b"riscv")
                        {
                            state.is_riscv_cpu = true;
                        }
                    }
                    else if name == b"reg" && prop_len >= 4
                    {
                        // #address-cells=1 under /cpus: reg is a single BE u32.
                        let data = self.struct_slice(data_off, 4);
                        if data.len() >= 4
                        {
                            state.reg_u32 =
                                u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                            state.has_reg = true;
                        }
                    }
                    else if name == b"status"
                    {
                        let data = self.struct_slice(data_off, prop_len);
                        // "disabled\0" means skip this CPU.
                        if data.starts_with(b"disabled")
                        {
                            state.disabled = true;
                        }
                    }
                }
                FDT_NOP =>
                {}
                // FDT_END = end of struct block; any other token = malformed/unknown.
                _ => break,
            }
        }
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Skip the null-terminated, 4-byte-aligned node name starting at `off` in
/// the struct block. Returns the updated offset after the name.
// `len` (usize) is bounded by `max` which equals `size_struct.saturating_sub(start)` (u32),
// so the `len as u32` cast below cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn skip_node_name(fdt: &Fdt, start: u32) -> u32
{
    let base_addr = fdt.base + u64::from(fdt.off_struct) + u64::from(start);
    let max = fdt.size_struct.saturating_sub(start) as usize;
    let mut len = 0;
    while len < max
    {
        // SAFETY: base_addr + len is within struct block bounds (len < max ≤ size_struct).
        let b = unsafe { core::ptr::read((base_addr + len as u64) as *const u8) };
        len += 1;
        if b == 0
        {
            break;
        }
    }
    // Round up to 4-byte alignment. `len ≤ max ≤ size_struct (u32::MAX)` so cast is exact.
    (start + len as u32 + 3) & !3
}

/// Check whether `data` (a null-separated compatible string list) contains
/// `target` as one of its entries.
fn prop_contains(data: &[u8], target: &[u8]) -> bool
{
    for s in data.split(|&b| b == 0)
    {
        if s == target
        {
            return true;
        }
    }
    false
}

/// Read a big-endian u32 from the first 4 bytes of `buf`.
/// Returns 0 if `buf` has fewer than 4 bytes.
fn read_be32(buf: &[u8]) -> u32
{
    if buf.len() < 4
    {
        return 0;
    }
    u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
}

/// Read a big-endian u64 from the first 8 bytes of `buf`.
/// Returns 0 if `buf` has fewer than 8 bytes.
fn read_be64(buf: &[u8]) -> u64
{
    if buf.len() < 8
    {
        return 0;
    }
    u64::from_be_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ])
}

// ── CPU topology parsing ──────────────────────────────────────────────────────

/// Parse DTB CPU nodes and return `(cpu_count, hart_ids)`.
///
/// Walks nodes with `compatible = "riscv"`, which is the standard compatible
/// string for RISC-V CPU nodes. Under `/cpus`, `#address-cells=1, #size-cells=0`
/// so the `reg` property is a single big-endian u32 hart ID.
///
/// The generic [`Fdt::walk_compatible`] assumes `#address-cells=2`, so this
/// function contains its own property reader for the u32 `reg` field.
///
/// Returns `(0, [0;64])` if no CPU nodes are found or DTB is invalid — the
/// caller falls back to ACPI or single-CPU operation.
///
/// # Safety
/// `dtb_addr` must be the physical address of a valid, identity-mapped FDT.
pub unsafe fn parse_cpu_count(dtb_addr: u64) -> (u32, [u32; 64])
{
    // SAFETY: caller guarantees dtb_addr is identity-mapped DTB.
    let Some(fdt) = (unsafe { Fdt::from_raw(dtb_addr) })
    else
    {
        return (0, [0u32; 64]);
    };

    let mut hart_ids = [0u32; 64];
    let mut count: u32 = 0;

    // Walk nodes compatible with "riscv". Each CPU node will have this as
    // a (prefix) compatible string. We extract reg as a single BE u32.
    fdt.walk_cpu_nodes(|reg_u32| {
        if count < 64
        {
            hart_ids[count as usize] = reg_u32;
            count += 1;
        }
        true // continue
    });

    (count, hart_ids)
}

// ── Public parsing functions ──────────────────────────────────────────────────

/// Parse DTB and extract [`PlatformResource`] entries into `out`.
///
/// Returns the number of entries written. Non-fatal on malformed nodes —
/// logs a warning and returns partial results.
///
/// Resources extracted:
/// - `riscv,plic0` / `sifive,plic-1.0.0`  → `MmioRange`
/// - `riscv,clint0` / `sifive,clint0`      → `MmioRange`
/// - `pci-host-ecam-generic`                → `PciEcam` + `MmioRange` (MMIO windows from `ranges`)
/// - `virtio,mmio`                          → `MmioRange` + `IrqLine`
/// - `ns16550a`                             → `MmioRange`
/// - DTB blob itself                        → `PlatformTable`
///
/// # Safety
/// `dtb_addr` must be the physical address of a valid, identity-mapped FDT.
// too_many_lines: resource extraction from multiple compatible strings is
// inherently sequential; splitting would add indirection without clarity.
#[allow(clippy::too_many_lines)]
pub unsafe fn parse_dtb_resources(dtb_addr: u64, out: &mut [PlatformResource]) -> usize
{
    // SAFETY: caller guarantees dtb_addr is identity-mapped DTB.
    let Some(fdt) = (unsafe { Fdt::from_raw(dtb_addr) })
    else
    {
        bprintln!("[--------] boot:     DTB: invalid magic, skipping");
        return 0;
    };

    let mut count = 0;

    /// Push a [`PlatformResource`] into `out` if space remains.
    macro_rules! push_resource {
        ($res:expr) => {
            if count < out.len()
            {
                out[count] = $res;
                count += 1;
            }
        };
    }

    // PLIC: riscv,plic0 or sifive,plic-1.0.0
    for compat in [b"riscv,plic0".as_ref(), b"sifive,plic-1.0.0".as_ref()]
    {
        fdt.for_each_compatible(compat, |node| {
            if node.reg_count > 0
            {
                push_resource!(PlatformResource {
                    resource_type: ResourceType::MmioRange,
                    flags: 0,
                    base: node.reg_entries[0].0,
                    size: node.reg_entries[0].1.max(4096),
                    id: 0,
                });
            }
        });
    }

    // CLINT: riscv,clint0 or sifive,clint0
    for compat in [b"riscv,clint0".as_ref(), b"sifive,clint0".as_ref()]
    {
        fdt.for_each_compatible(compat, |node| {
            if node.reg_count > 0
            {
                push_resource!(PlatformResource {
                    resource_type: ResourceType::MmioRange,
                    flags: 0,
                    base: node.reg_entries[0].0,
                    size: node.reg_entries[0].1.max(4096),
                    id: 0,
                });
            }
        });
    }

    // PCIe ECAM: pci-host-ecam-generic
    // flags = (start_bus=0) | (end_bus=255 << 8) = 0xFF00 (default full range).
    // To-do: parse `bus-range` property for accurate values.
    fdt.for_each_compatible(b"pci-host-ecam-generic", |node| {
        if node.reg_count > 0
        {
            push_resource!(PlatformResource {
                resource_type: ResourceType::PciEcam,
                flags: 0xFF00, // start_bus=0, end_bus=255
                base: node.reg_entries[0].0,
                size: node.reg_entries[0].1.max(4096),
                id: 0,
            });
        }

        // PCI MMIO windows from `ranges` property.
        // child_flags bits 25:24 encode space type: 2 = 32-bit MMIO, 3 = 64-bit MMIO.
        for i in 0..node.ranges_count
        {
            let (flags, cpu_addr, size) = node.ranges_entries[i];
            let space_type = (flags >> 24) & 0x03;
            if space_type == 2 || space_type == 3
            {
                push_resource!(PlatformResource {
                    resource_type: ResourceType::MmioRange,
                    flags: 0,
                    base: cpu_addr,
                    size,
                    id: 0,
                });
            }
        }
    });

    // VirtIO MMIO devices: virtio,mmio
    fdt.for_each_compatible(b"virtio,mmio", |node| {
        if node.reg_count > 0
        {
            push_resource!(PlatformResource {
                resource_type: ResourceType::MmioRange,
                flags: 0,
                base: node.reg_entries[0].0,
                size: node.reg_entries[0].1.max(4096),
                id: 0,
            });
        }
        // Emit interrupt line for this device.
        if node.irq_count > 0
        {
            push_resource!(PlatformResource {
                resource_type: ResourceType::IrqLine,
                flags: 0, // edge/level determined by platform
                base: 0,
                size: 0,
                id: u64::from(node.irq_entries[0]),
            });
        }
    });

    // UART: ns16550a
    fdt.for_each_compatible(b"ns16550a", |node| {
        if node.reg_count > 0
        {
            push_resource!(PlatformResource {
                resource_type: ResourceType::MmioRange,
                flags: 0,
                base: node.reg_entries[0].0,
                size: node.reg_entries[0].1.max(4096),
                id: 0,
            });
        }
    });

    // DTB blob itself → PlatformTable (id=0: DTB).
    push_resource!(PlatformResource {
        resource_type: ResourceType::PlatformTable,
        flags: 0,
        base: dtb_addr,
        size: u64::from(fdt.total_size()),
        id: 0,
    });

    count
}
