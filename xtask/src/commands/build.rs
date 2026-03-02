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

/// Entry point for `cargo xtask build`.
pub fn run(ctx: &BuildContext, args: &BuildArgs) -> Result<()>
{
    sysroot::check_arch(ctx, args.arch)?;

    match args.component
    {
        BuildComponent::Boot => build_boot(ctx, args)?,
        BuildComponent::Kernel => build_kernel(ctx, args)?,
        BuildComponent::Init => build_init(ctx, args)?,
        BuildComponent::All =>
        {
            build_boot(ctx, args)?;
            build_kernel(ctx, args)?;
            build_init(ctx, args)?;
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
        "seraph-boot",
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
            let elf_out = ctx
                .cargo_output_dir(boot_triple, args.release)
                .join("seraph-boot");
            if !elf_out.exists()
            {
                bail!("expected ELF output not found: {}", elf_out.display());
            }

            let objcopy = find_llvm_objcopy()?;
            let dst_efi = efi_boot_dir.join(efi_name);
            run_cmd(
                Command::new(&objcopy)
                    .args(["-O", "binary"])
                    .arg(&elf_out)
                    .arg(&dst_efi),
            )?;
            copy_file(&dst_efi, &efi_seraph_dir.join("boot"))?;

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
                .join("seraph-boot.efi");
            if !cargo_out.exists()
            {
                bail!("expected EFI output not found: {}", cargo_out.display());
            }

            let dst_efi = efi_boot_dir.join(efi_name);
            copy_file(&cargo_out, &dst_efi)?;
            copy_file(&cargo_out, &efi_seraph_dir.join("boot"))?;

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
        "seraph-kernel",
        "--bin",
        "seraph-kernel",
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

    let cargo_out = ctx
        .cargo_output_dir(triple, args.release)
        .join("seraph-kernel");
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
        "seraph-init",
        "--bin",
        "seraph-init",
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

    let cargo_out = ctx
        .cargo_output_dir(triple, args.release)
        .join("seraph-init");
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
