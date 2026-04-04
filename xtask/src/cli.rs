// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

//! cli.rs
//!
//! Clap derive structs for the xtask CLI.
//!
//! Add a new top-level command by adding a variant to `Command` and a
//! corresponding `Args` struct below, then handle it in `main.rs`.

use clap::{Parser, Subcommand, ValueEnum};

use crate::arch::Arch;

/// Top-level CLI entry point.
#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Seraph build task runner — invoke via `cargo xtask`"
)]
pub struct Cli
{
    #[command(subcommand)]
    pub command: CliCommand,
}

/// Available subcommands.
#[derive(Subcommand)]
pub enum CliCommand
{
    /// Build Seraph components and populate the sysroot.
    Build(BuildArgs),

    /// Build (if needed) and launch Seraph under QEMU.
    Run(RunArgs),

    /// Remove the sysroot (and optionally cargo target/).
    Clean(CleanArgs),

    /// Run Seraph unit tests on the host.
    Test(TestArgs),
}

// ── Build ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
pub struct BuildArgs
{
    /// Target architecture.
    #[arg(long, default_value = "x86_64")]
    pub arch: Arch,

    /// Build in release mode (default: debug).
    #[arg(long)]
    pub release: bool,

    /// Build only one component (default: all).
    #[arg(long, default_value = "all")]
    pub component: BuildComponent,
}

/// Components that can be built individually.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BuildComponent
{
    Boot,
    Kernel,
    Init,
    Ktest,
    Procmgr,
    Devmgr,
    Vfsd,
    VirtioBlk,
    Fatfs,
    All,
}

// ── Run ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
pub struct RunArgs
{
    /// Target architecture.
    #[arg(long, default_value = "x86_64")]
    pub arch: Arch,

    /// Use the release build.
    #[arg(long)]
    pub release: bool,

    /// Start QEMU with a GDB server on localhost:1234 (QEMU pauses at startup).
    ///
    /// KVM is disabled in GDB mode so register reads and breakpoints work correctly.
    /// TCG is ~5–10x slower; expect ~30s to reach the bootloader instead of ~5s.
    #[arg(long)]
    pub gdb: bool,

    /// Run without a display window (-display none).
    #[arg(long)]
    pub headless: bool,

    /// Show all serial output including pre-boot firmware noise (filtered by default).
    ///
    /// By default, output is suppressed until the first line containing
    /// '[--------] boot:', hiding UEFI/OpenSBI debug spam.
    #[arg(long)]
    pub verbose: bool,

    /// Number of vCPUs to expose to the guest (QEMU -smp).
    #[arg(long, default_value = "4")]
    pub cpus: u32,
}

// ── Clean ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
pub struct CleanArgs
{
    /// Also remove the cargo target/ directory (full clean).
    #[arg(long)]
    pub all: bool,
}

// ── Test ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
pub struct TestArgs
{
    /// Test a single component (default: all).
    #[arg(long, default_value = "all")]
    pub component: TestComponent,
}

/// Components that can be tested individually.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum TestComponent
{
    Boot,
    Protocol,
    Kernel,
    Init,
    All,
}
