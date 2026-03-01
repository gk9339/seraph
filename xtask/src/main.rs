// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

//! xtask — build task runner for Seraph.
//!
//! Run via `cargo xtask <command>`. This replaces the shell scripts in
//! `scripts/` once the build requires disk image assembly, multi-stage
//! builds, or integration test orchestration.
//!
//! # Adding a new command
//!
//! 1. Add a variant to the match below.
//! 2. Implement the command as a function in this file or a submodule.
//! 3. Update the usage string.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str);

    match command {
        None | Some("help") | Some("--help") | Some("-h") => {
            println!("Usage: cargo xtask <command>");
            println!();
            println!("Commands:");
            println!("  help    Print this message");
            println!();
            println!("No commands are implemented yet. See scripts/ for the current build interface.");
        }
        Some(cmd) => {
            eprintln!("error: unknown command '{cmd}'");
            eprintln!("Run 'cargo xtask help' for usage.");
            std::process::exit(1);
        }
    }
}
