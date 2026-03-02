// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

//! commands/build.rs
//!
//! Build command: cross-compile Seraph components and populate the sysroot.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::arch::Arch;
use crate::cli::{BuildArgs, BuildComponent};
use crate::context::Context as BuildContext;
use crate::sysroot;
use crate::util::{find_llvm_objcopy, run_cmd, step};

// Modules built with the kernel target triple, placed under EFI/seraph/.
// All modules follow the same build pattern: `-p <name> --bin <name>`, output
// at target/<triple>/<profile>/<name>, sysroot dest EFI/seraph/<name>.
//
// TODO: rework to support per-module configuration (different output paths,
// target triples, sysroot destinations, extra build flags). For now every
// module uses the same kernel target and flags.
const MODULES: &[&str] = &["procmgr", "devmgr", "vfsd", "virtio-blk", "fatfs"];

/// Entry point for `cargo xtask build`.
pub fn run(ctx: &BuildContext, args: &BuildArgs) -> Result<()>
{
    sysroot::check_arch(ctx, args.arch)?;

    match args.component
    {
        BuildComponent::Boot => build_boot(ctx, args)?,
        BuildComponent::Kernel => build_kernel(ctx, args)?,
        BuildComponent::Init => build_init(ctx, args)?,
        BuildComponent::Procmgr => build_module(ctx, args, "procmgr")?,
        BuildComponent::Devmgr => build_module(ctx, args, "devmgr")?,
        BuildComponent::Vfsd => build_module(ctx, args, "vfsd")?,
        BuildComponent::VirtioBlk => build_module(ctx, args, "virtio-blk")?,
        BuildComponent::Fatfs => build_module(ctx, args, "fatfs")?,
        BuildComponent::All =>
        {
            build_boot(ctx, args)?;
            build_kernel(ctx, args)?;
            build_init(ctx, args)?;
            build_modules(ctx, args)?;
            sysroot::install_rootfs(ctx)?;
        }
    }

    sysroot::record_arch(ctx, args.arch)?;
    let profile = profile_name(args.release);
    step(&format!("Build complete ({}, {})", args.arch, profile));
    Ok(())
}

// ── Component builders ────────────────────────────────────────────────────────

fn build_boot(ctx: &BuildContext, args: &BuildArgs) -> Result<()>
{
    step(&format!(
        "Building bootloader for {} ({})",
        args.arch,
        profile_name(args.release)
    ));

    let boot_triple = args.arch.boot_target_triple();
    let efi_name = args.arch.boot_efi_filename();

    let mut cmd = cargo(&ctx.root);
    cmd.args([
        "build",
        "-p",
        "boot",
        "--target",
        boot_triple,
        "-Zbuild-std=core,compiler_builtins",
        "-Zbuild-std-features=compiler-builtins-mem",
    ]);
    if args.release
    {
        cmd.arg("--release");
    }
    run_cmd(&mut cmd)?;

    let efi_boot_dir = ctx.sysroot_efi_boot();
    let efi_seraph_dir = ctx.sysroot_efi_seraph();
    fs::create_dir_all(&efi_boot_dir)
        .with_context(|| format!("creating {}", efi_boot_dir.display()))?;
    fs::create_dir_all(&efi_seraph_dir)
        .with_context(|| format!("creating {}", efi_seraph_dir.display()))?;

    match args.arch
    {
        Arch::Riscv64 =>
        {
            // RISC-V: cargo produces an ELF; convert to a flat PE32+ binary via
            // llvm-objcopy. The UEFI spec requires a PE32+ image on disk.
            let elf_out = ctx.cargo_output_dir(boot_triple, args.release).join("boot");
            if !elf_out.exists()
            {
                bail!("expected ELF output not found: {}", elf_out.display());
            }

            let objcopy = find_llvm_objcopy()?;
            let dst_boot = efi_seraph_dir.join("boot.efi");
            run_cmd(
                Command::new(&objcopy)
                    .args(["-O", "binary"])
                    .arg(&elf_out)
                    .arg(&dst_boot),
            )?;
            let dst_efi = efi_boot_dir.join(efi_name);
            copy_file(&dst_boot, &dst_efi)?;

            step(&format!(
                "Bootloader: {} (ELF → flat binary)",
                dst_efi.display()
            ));
            step(&format!(
                "Bootloader: {}",
                efi_seraph_dir.join("boot").display()
            ));
        }
        _ =>
        {
            // x86_64 (and future PE-native archs): cargo produces a .efi PE directly.
            let cargo_out = ctx
                .cargo_output_dir(boot_triple, args.release)
                .join("boot.efi");
            if !cargo_out.exists()
            {
                bail!("expected EFI output not found: {}", cargo_out.display());
            }

            let dst_boot = efi_seraph_dir.join("boot.efi");
            copy_file(&cargo_out, &dst_boot)?;
            let dst_efi = efi_boot_dir.join(efi_name);
            copy_file(&dst_boot, &dst_efi)?;

            step(&format!("Bootloader: {}", dst_efi.display()));
            step(&format!(
                "Bootloader: {}",
                efi_seraph_dir.join("boot").display()
            ));
        }
    }

    Ok(())
}

fn build_kernel(ctx: &BuildContext, args: &BuildArgs) -> Result<()>
{
    step(&format!(
        "Building kernel for {} ({})",
        args.arch,
        profile_name(args.release)
    ));

    let triple = args.arch.kernel_target_triple();
    let mut cmd = cargo(&ctx.root);
    cmd.args([
        "build",
        "-p",
        "kernel",
        "--bin",
        "kernel",
        "--target",
        triple,
        "-Zbuild-std=core,alloc,compiler_builtins",
        "-Zbuild-std-features=compiler-builtins-mem",
    ]);
    if args.release
    {
        cmd.arg("--release");
    }
    run_cmd(&mut cmd)?;

    let cargo_out = ctx.cargo_output_dir(triple, args.release).join("kernel");
    if !cargo_out.exists()
    {
        bail!("expected kernel binary not found: {}", cargo_out.display());
    }

    let dst = ctx.sysroot_efi_seraph().join("kernel");
    fs::create_dir_all(ctx.sysroot_efi_seraph())
        .with_context(|| format!("creating {}", ctx.sysroot_efi_seraph().display()))?;
    copy_file(&cargo_out, &dst)?;
    step(&format!("Kernel: {}", dst.display()));

    Ok(())
}

fn build_init(ctx: &BuildContext, args: &BuildArgs) -> Result<()>
{
    step(&format!(
        "Building init for {} ({})",
        args.arch,
        profile_name(args.release)
    ));

    let triple = args.arch.kernel_target_triple();
    let mut cmd = cargo(&ctx.root);
    cmd.args([
        "build",
        "-p",
        "init",
        "--bin",
        "init",
        "--target",
        triple,
        "-Zbuild-std=core,compiler_builtins",
        "-Zbuild-std-features=compiler-builtins-mem",
    ]);
    if args.release
    {
        cmd.arg("--release");
    }
    run_cmd(&mut cmd)?;

    let cargo_out = ctx.cargo_output_dir(triple, args.release).join("init");
    if !cargo_out.exists()
    {
        bail!("expected init binary not found: {}", cargo_out.display());
    }

    let dst = ctx.sysroot_efi_seraph().join("init");
    fs::create_dir_all(ctx.sysroot_efi_seraph())
        .with_context(|| format!("creating {}", ctx.sysroot_efi_seraph().display()))?;
    copy_file(&cargo_out, &dst)?;
    step(&format!("Init: {}", dst.display()));

    Ok(())
}

/// Build a single module and copy it to the sysroot.
///
/// All modules use the same kernel target triple and build flags. `name` must
/// match the crate's `package.name` in Cargo.toml and the desired sysroot
/// filename under `EFI/seraph/`.
///
/// If a module ever needs special treatment (different target, extra flags,
/// different sysroot path), extract it into its own build function — same
/// pattern as `build_boot`, `build_kernel`, and `build_init` above.
fn build_module(ctx: &BuildContext, args: &BuildArgs, name: &str) -> Result<()>
{
    step(&format!(
        "Building {} for {} ({})",
        name,
        args.arch,
        profile_name(args.release)
    ));

    let triple = args.arch.kernel_target_triple();
    let mut cmd = cargo(&ctx.root);
    cmd.args([
        "build",
        "-p",
        name,
        "--bin",
        name,
        "--target",
        triple,
        "-Zbuild-std=core,compiler_builtins",
        "-Zbuild-std-features=compiler-builtins-mem",
    ]);
    if args.release
    {
        cmd.arg("--release");
    }
    run_cmd(&mut cmd)?;

    let cargo_out = ctx.cargo_output_dir(triple, args.release).join(name);
    if !cargo_out.exists()
    {
        bail!(
            "expected {} binary not found: {}",
            name,
            cargo_out.display()
        );
    }

    let dst = ctx.sysroot_efi_seraph().join(name);
    fs::create_dir_all(ctx.sysroot_efi_seraph())
        .with_context(|| format!("creating {}", ctx.sysroot_efi_seraph().display()))?;
    copy_file(&cargo_out, &dst)?;
    step(&format!("{}: {}", name, dst.display()));

    Ok(())
}

/// Build all modules listed in `MODULES`.
fn build_modules(ctx: &BuildContext, args: &BuildArgs) -> Result<()>
{
    for &name in MODULES
    {
        build_module(ctx, args, name)?;
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Construct a `cargo` Command with the working directory set to the workspace root.
fn cargo(root: &Path) -> Command
{
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root);
    cmd
}

/// Convenience wrapper for `fs::copy` with a context-annotated error.
fn copy_file(src: &Path, dst: &Path) -> Result<()>
{
    fs::copy(src, dst)
        .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

/// Human-readable profile name matching Cargo's output directory naming.
fn profile_name(release: bool) -> &'static str
{
    if release
    {
        "release"
    }
    else
    {
        "debug"
    }
}
