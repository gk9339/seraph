// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

//! context.rs
//!
//! Resolved workspace and build output paths, shared by all commands.

use std::path::PathBuf;

/// Resolved project paths used by every command.
pub struct Context
{
    /// Absolute path to the workspace root (parent of xtask/).
    pub root: PathBuf,

    /// Sysroot staging area: used directly as a virtual FAT drive by QEMU.
    /// Populated by `cargo xtask build`, cleared by `cargo xtask clean`.
    pub sysroot: PathBuf,

    /// Cargo's `target/` directory, shared by all workspace members.
    pub target_dir: PathBuf,
}

impl Context
{
    /// Construct a Context from the compiled-in xtask manifest directory.
    ///
    /// `CARGO_MANIFEST_DIR` is set by Cargo to the xtask crate root at compile
    /// time; its parent is the workspace root.
    pub fn from_manifest_dir() -> Self
    {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir
            .parent()
            .expect("xtask manifest dir has no parent — check workspace layout")
            .to_path_buf();
        let sysroot = root.join("sysroot");
        let target_dir = root.join("target");
        Context { root, sysroot, target_dir }
    }

    /// Path to `sysroot/EFI/BOOT/` — where the UEFI fallback bootloader lives.
    pub fn sysroot_efi_boot(&self) -> PathBuf
    {
        self.sysroot.join("EFI").join("BOOT")
    }

    /// Path to `sysroot/EFI/seraph/` — Seraph's vendor directory on the ESP.
    pub fn sysroot_efi_seraph(&self) -> PathBuf
    {
        self.sysroot.join("EFI").join("seraph")
    }

    /// Cargo output directory for a given target triple and build profile.
    ///
    /// Cargo outputs the `dev` profile to `debug/`; all other profiles use the
    /// profile name as the directory name.
    pub fn cargo_output_dir(&self, triple: &str, release: bool) -> PathBuf
    {
        let profile_dir = if release { "release" } else { "debug" };
        self.target_dir.join(triple).join(profile_dir)
    }
}
