#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Copyright (C) 2026 George Kottler <mail@kottlerg.com>

# scripts/env.sh

# Shared environment for Seraph build scripts. Source this file from
# other scripts; do not execute it directly.

set -euo pipefail

# Absolute path to the repository root.
SERAPH_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Build output directory (the sysroot staging area).
SERAPH_SYSROOT="${SERAPH_ROOT}/sysroot"

# Custom target JSON directory.
SERAPH_TARGETS_DIR="${SERAPH_ROOT}/scripts/targets"

# Default architecture if not specified by the caller.
SERAPH_ARCH="${SERAPH_ARCH:-x86_64}"

# ── Helper functions ──────────────────────────────────────────────────────────

# Print a step message to stdout.
step()
{
    echo "==> $*"
}

# Print an error message to stderr and exit.
die()
{
    echo "error: $*" >&2
    exit 1
}

# Validate that an architecture name is supported.
validate_arch()
{
    case "$1" in
        x86_64|riscv64)
            ;;
        *)
            die "unsupported architecture '$1' (supported: x86_64, riscv64)"
            ;;
    esac
}

# Map an arch name to the Rust target triple used by the kernel.
kernel_target_triple()
{
    case "$1" in
        x86_64)  echo "x86_64-seraph-none" ;;
        riscv64) echo "riscv64gc-seraph-none" ;;
    esac
}

# Absolute path to the kernel's custom target JSON for the given arch.
kernel_target_json()
{
    local triple
    triple="$(kernel_target_triple "$1")"
    echo "${SERAPH_TARGETS_DIR}/${triple}.json"
}

# Map an arch name to the Rust target triple used by the bootloader.
boot_target_triple()
{
    case "$1" in
        x86_64)  echo "x86_64-unknown-uefi" ;;
        riscv64) echo "riscv64gc-seraph-uefi" ;;
    esac
}

# Absolute path to the bootloader's custom target JSON for the given arch.
# Prints nothing for arches that use a built-in Rust target (e.g. x86_64).
boot_target_json()
{
    case "$1" in
        x86_64)  echo "" ;;
        riscv64) echo "${SERAPH_TARGETS_DIR}/riscv64gc-seraph-uefi.json" ;;
    esac
}

# Return 0 (true) if the bootloader for the given arch requires a post-build
# objcopy step to convert the ELF to a flat PE32+ binary.
boot_needs_objcopy()
{
    case "$1" in
        x86_64)  return 1 ;;
        riscv64) return 0 ;;
    esac
}

# Locate llvm-objcopy from the active Rust toolchain's llvm-tools component.
# Exits with an error if not found.
find_llvm_objcopy()
{
    local sysroot host_triple objcopy
    sysroot="$(rustc --print sysroot)"
    host_triple="$(rustc -vV | sed -n 's/^host: //p')"
    objcopy="${sysroot}/lib/rustlib/${host_triple}/bin/llvm-objcopy"
    if [[ -x "${objcopy}" ]]
    then
        echo "${objcopy}"
    else
        die "llvm-objcopy not found in toolchain sysroot (${objcopy})." \
            $'\n       Install the llvm-tools component: rustup component add llvm-tools'
    fi
}

# Map an arch name to the UEFI fallback bootloader filename (EFI/BOOT/<name>).
# This is the filename UEFI firmware looks for when no explicit boot entry exists.
boot_efi_filename()
{
    case "$1" in
        x86_64)  echo "BOOTX64.EFI" ;;
        riscv64) echo "BOOTRISCV64.EFI" ;;
    esac
}

# Check that the sysroot is either empty or was built for the given arch.
# Prints an error and exits if there is a mismatch.
check_sysroot_arch()
{
    local arch="$1"
    local arch_file="${SERAPH_SYSROOT}/.arch"

    if [[ -f "${arch_file}" ]]
    then
        local existing_arch
        existing_arch="$(cat "${arch_file}")"
        if [[ "${existing_arch}" != "${arch}" ]]
        then
            die "sysroot was built for '${existing_arch}', not '${arch}'." \
                $'\n       Run ./clean.sh before switching architectures.'
        fi
    fi
}

# Record the active architecture in the sysroot.
record_sysroot_arch()
{
    local arch="$1"
    mkdir -p "${SERAPH_SYSROOT}"
    echo "${arch}" > "${SERAPH_SYSROOT}/.arch"
}
