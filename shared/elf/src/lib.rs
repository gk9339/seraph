// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/elf/src/lib.rs

//! Shared ELF64 parsing crate for Seraph userspace components.
//!
//! Provides validation, header parsing, and `PT_LOAD` segment enumeration for
//! ELF64 static executables. Used by init (to load procmgr from a boot module)
//! and by procmgr (to load all subsequent processes).
//!
//! This crate is `no_std` and performs no allocation or I/O. All functions
//! operate on a byte slice representing the raw ELF image.

#![no_std]

// cast_possible_truncation: this crate targets 64-bit only (x86-64, riscv64);
// ELF64 u64 fields fit in usize on these platforms.
#![allow(clippy::cast_possible_truncation)]

use core::mem::size_of;

// ── ELF identification constants ─────────────────────────────────────────────

const ELFMAG0: u8 = 0x7F;
const ELFMAG1: u8 = b'E';
const ELFMAG2: u8 = b'L';
const ELFMAG3: u8 = b'F';

/// ELF class: 64-bit object.
const ELFCLASS64: u8 = 2;
/// ELF data encoding: 2's complement, little-endian.
const ELFDATA2LSB: u8 = 1;
/// ELF version: current (1).
const EV_CURRENT: u8 = 1;

/// ELF type: static executable.
const ET_EXEC: u16 = 2;

/// Machine type: x86-64.
pub const EM_X86_64: u16 = 0x3E;
/// Machine type: RISC-V.
pub const EM_RISCV: u16 = 0xF3;

// ── Program header constants ─────────────────────────────────────────────────

/// Program header type: loadable segment.
const PT_LOAD: u32 = 1;

/// Segment flag: execute permission.
const PF_X: u32 = 1;
/// Segment flag: write permission.
const PF_W: u32 = 2;

// ── e_ident index constants ──────────────────────────────────────────────────

const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const EI_VERSION: usize = 6;

// ── Error type ───────────────────────────────────────────────────────────────

/// Errors returned by ELF validation and parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError
{
    /// Data too small to contain the ELF header.
    TooSmall,
    /// ELF magic bytes do not match.
    BadMagic,
    /// Not a 64-bit ELF (`ELFCLASS64`).
    Not64Bit,
    /// Not little-endian (`ELFDATA2LSB`).
    NotLittleEndian,
    /// ELF version is not `EV_CURRENT`.
    BadVersion,
    /// Not a static executable (`ET_EXEC`).
    NotExecutable,
    /// Machine type does not match the expected architecture.
    WrongMachine,
    /// Program header entry size does not match `Elf64Phdr`.
    BadPhentsize,
    /// No program headers present.
    NoSegments,
    /// Program header table extends beyond the file data.
    PhdrTableOverflow,
    /// A `PT_LOAD` segment references data beyond the file.
    SegmentOverflow,
}

// ── Raw ELF types ────────────────────────────────────────────────────────────

/// 64-bit ELF file header (`Elf64_Ehdr`).
#[repr(C)]
// ELF field names follow the ELF spec (e_ident, e_type, …); removing the `e_`
// prefix would diverge from all reference material.
#[allow(clippy::struct_field_names)]
pub struct Elf64Ehdr
{
    /// Magic number and ELF identification fields.
    pub e_ident: [u8; 16],
    /// Object file type (e.g. `ET_EXEC`).
    pub e_type: u16,
    /// Target machine architecture.
    pub e_machine: u16,
    /// ELF format version; must equal `EV_CURRENT`.
    pub e_version: u32,
    /// Virtual address of the program entry point.
    pub e_entry: u64,
    /// File offset of the program header table.
    pub e_phoff: u64,
    /// File offset of the section header table (unused).
    pub e_shoff: u64,
    /// Processor-specific flags.
    pub e_flags: u32,
    /// Size of this header in bytes.
    pub e_ehsize: u16,
    /// Size of one program header entry in bytes.
    pub e_phentsize: u16,
    /// Number of program header entries.
    pub e_phnum: u16,
    /// Size of one section header entry in bytes (unused).
    pub e_shentsize: u16,
    /// Number of section header entries (unused).
    pub e_shnum: u16,
    /// Index of the section name string table entry (unused).
    pub e_shstrndx: u16,
}

/// 64-bit ELF program header (`Elf64_Phdr`).
#[repr(C)]
// ELF field names follow the ELF spec (p_type, p_flags, …).
#[allow(clippy::struct_field_names)]
pub struct Elf64Phdr
{
    /// Segment type (e.g. `PT_LOAD`).
    pub p_type: u32,
    /// Segment-dependent permission flags (`PF_R`, `PF_W`, `PF_X`).
    pub p_flags: u32,
    /// Byte offset of the segment data within the file.
    pub p_offset: u64,
    /// Virtual address at which the segment is to be loaded.
    pub p_vaddr: u64,
    /// Physical address of the segment (unused for userspace loading).
    pub p_paddr: u64,
    /// Number of bytes in the file image of the segment.
    pub p_filesz: u64,
    /// Number of bytes in the memory image (may exceed `p_filesz` for BSS).
    pub p_memsz: u64,
    /// Required alignment; must be a power of two, or zero.
    pub p_align: u64,
}

// ── Output types ─────────────────────────────────────────────────────────────

/// A `PT_LOAD` segment extracted from an ELF image.
///
/// Describes where to find the segment data in the raw ELF byte slice and
/// where to place it in the target address space.
#[derive(Debug, Clone, Copy)]
pub struct LoadSegment
{
    /// Virtual address at which this segment must be mapped.
    pub vaddr: u64,
    /// Byte offset within the ELF file where segment data starts.
    pub offset: u64,
    /// Number of bytes of file data to copy (`p_filesz`).
    pub filesz: u64,
    /// Total size in memory (`p_memsz`). Bytes beyond `filesz` are zero (BSS).
    pub memsz: u64,
    /// Segment is writable.
    pub writable: bool,
    /// Segment is executable.
    pub executable: bool,
}

// ── Validation ───────────────────────────────────────────────────────────────

/// Validate an ELF64 header and return a typed reference.
///
/// Checks: minimum size, magic, class (64-bit), data encoding (little-endian),
/// version, type (`ET_EXEC`), machine, program header entry size, and program
/// header count.
///
/// # Errors
///
/// Returns an [`ElfError`] variant for each failed check.
pub fn validate(data: &[u8], expected_machine: u16) -> Result<&Elf64Ehdr, ElfError>
{
    if data.len() < size_of::<Elf64Ehdr>()
    {
        return Err(ElfError::TooSmall);
    }

    // SAFETY: length check above guarantees data is large enough for Elf64Ehdr.
    // cast_ptr_alignment: ELF files from the bootloader are loaded at
    // page-aligned physical addresses; the u8 slice spans that region.
    #[allow(clippy::cast_ptr_alignment)]
    let ehdr = unsafe { &*data.as_ptr().cast::<Elf64Ehdr>() };

    if ehdr.e_ident[0] != ELFMAG0
        || ehdr.e_ident[1] != ELFMAG1
        || ehdr.e_ident[2] != ELFMAG2
        || ehdr.e_ident[3] != ELFMAG3
    {
        return Err(ElfError::BadMagic);
    }
    if ehdr.e_ident[EI_CLASS] != ELFCLASS64
    {
        return Err(ElfError::Not64Bit);
    }
    if ehdr.e_ident[EI_DATA] != ELFDATA2LSB
    {
        return Err(ElfError::NotLittleEndian);
    }
    if ehdr.e_ident[EI_VERSION] != EV_CURRENT
    {
        return Err(ElfError::BadVersion);
    }
    if ehdr.e_type != ET_EXEC
    {
        return Err(ElfError::NotExecutable);
    }
    if ehdr.e_machine != expected_machine
    {
        return Err(ElfError::WrongMachine);
    }
    if ehdr.e_phentsize as usize != size_of::<Elf64Phdr>()
    {
        return Err(ElfError::BadPhentsize);
    }
    if ehdr.e_phnum == 0
    {
        return Err(ElfError::NoSegments);
    }

    // Verify the program header table fits within the file data.
    let phdr_end = (ehdr.e_phoff as usize)
        .checked_add(ehdr.e_phnum as usize * size_of::<Elf64Phdr>())
        .ok_or(ElfError::PhdrTableOverflow)?;
    if phdr_end > data.len()
    {
        return Err(ElfError::PhdrTableOverflow);
    }

    Ok(ehdr)
}

/// Return the entry point virtual address from a validated ELF header.
#[must_use]
pub fn entry_point(ehdr: &Elf64Ehdr) -> u64
{
    ehdr.e_entry
}

// ── Segment iteration ────────────────────────────────────────────────────────

/// Iterator over `PT_LOAD` segments in a validated ELF image.
///
/// Created by [`load_segments`]. Yields [`LoadSegment`] values for each
/// `PT_LOAD` program header, skipping all other segment types.
pub struct LoadSegmentIter<'a>
{
    data: &'a [u8],
    phdr_base: usize,
    phdr_count: usize,
    index: usize,
}

impl Iterator for LoadSegmentIter<'_>
{
    type Item = Result<LoadSegment, ElfError>;

    fn next(&mut self) -> Option<Self::Item>
    {
        while self.index < self.phdr_count
        {
            let offset = self.phdr_base + self.index * size_of::<Elf64Phdr>();
            self.index += 1;

            // SAFETY: validate() confirmed the entire program header table
            // fits within data. Each entry is at a known offset.
            // cast_ptr_alignment: see validate() — ELF data is page-aligned.
            #[allow(clippy::cast_ptr_alignment)]
            let phdr = unsafe { &*self.data.as_ptr().add(offset).cast::<Elf64Phdr>() };

            if phdr.p_type != PT_LOAD
            {
                continue;
            }

            // Validate that the file data for this segment is in bounds.
            let seg_end = (phdr.p_offset as usize)
                .checked_add(phdr.p_filesz as usize);
            match seg_end
            {
                Some(end) if end <= self.data.len() => {}
                _ => return Some(Err(ElfError::SegmentOverflow)),
            }

            return Some(Ok(LoadSegment {
                vaddr: phdr.p_vaddr,
                offset: phdr.p_offset,
                filesz: phdr.p_filesz,
                memsz: phdr.p_memsz,
                writable: phdr.p_flags & PF_W != 0,
                executable: phdr.p_flags & PF_X != 0,
            }));
        }
        None
    }
}

/// Return an iterator over `PT_LOAD` segments in a validated ELF image.
///
/// `ehdr` must have been returned by a successful [`validate`] call on `data`.
/// The iterator yields one [`LoadSegment`] per `PT_LOAD` program header,
/// skipping all other segment types.
///
/// # Errors
///
/// Individual segments may yield `Err(`[`ElfError::SegmentOverflow`]`)` if
/// their file data extends beyond the ELF image.
#[must_use]
pub fn load_segments<'a>(ehdr: &Elf64Ehdr, data: &'a [u8]) -> LoadSegmentIter<'a>
{
    LoadSegmentIter {
        data,
        phdr_base: ehdr.e_phoff as usize,
        phdr_count: ehdr.e_phnum as usize,
        index: 0,
    }
}
