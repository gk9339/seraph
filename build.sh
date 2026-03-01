#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Copyright (C) 2026 George Kottler <mail@kottlerg.com>

# build.sh

# Build Seraph components.
#
# Usage:
#   ./build.sh [OPTIONS]
#
# Options:
#   --arch ARCH           Target architecture: x86_64 (default), riscv64
#   --release             Build in release mode (default: debug)
#   --component COMPONENT Build only one component: boot, kernel, init, all (default: all)
#   -h, --help            Show this help and exit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/env.sh
source "${SCRIPT_DIR}/scripts/env.sh"

ARCH="${SERAPH_ARCH}"
PROFILE="dev"
# Cargo outputs the dev profile to "debug/", not "dev/".
OUTPUT_DIR="debug"
CARGO_PROFILE_FLAG=""
COMPONENT="all"

while [[ $# -gt 0 ]]
do
    case "$1" in
        --arch)
            ARCH="$2"; shift 2
            ;;
        --release)
            PROFILE="release"
            OUTPUT_DIR="release"
            CARGO_PROFILE_FLAG="--release"
            shift
            ;;
        --component)
            COMPONENT="$2"; shift 2
            ;;
        -h|--help)
            sed -n '7,16p' "$0"
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

validate_arch "${ARCH}"
check_sysroot_arch "${ARCH}"

KERNEL_TRIPLE="$(kernel_target_triple "${ARCH}")"

SYSROOT_EFI_SERAPH="${SERAPH_SYSROOT}/EFI/seraph"

# ── Component build functions ─────────────────────────────────────────────────

build_boot()
{
    local boot_triple efi_name
    boot_triple="$(boot_target_triple "${ARCH}")"
    efi_name="$(boot_efi_filename "${ARCH}")"

    step "Building bootloader for ${ARCH} (${PROFILE})"

    # Target JSON is resolved by rustc via RUST_TARGET_PATH (set in .cargo/config.toml).
    # Linker flags for RISC-V are also in .cargo/config.toml under [target.*].
    # shellcheck disable=SC2086
    cargo build \
        --manifest-path "${SERAPH_ROOT}/boot/Cargo.toml" \
        --target "${boot_triple}" \
        -Zbuild-std=core,compiler_builtins \
        -Zbuild-std-features=compiler-builtins-mem \
        ${CARGO_PROFILE_FLAG}

    local efi_dir="${SERAPH_SYSROOT}/EFI/BOOT"
    mkdir -p "${efi_dir}"

    if boot_needs_objcopy "${ARCH}"
    then
        # RISC-V: cargo produces an ELF; convert to a flat PE32+ binary.
        local elf_out="${SERAPH_ROOT}/target/${boot_triple}/${OUTPUT_DIR}/seraph-boot"
        local objcopy
        objcopy="$(find_llvm_objcopy)"

        if [[ ! -f "${elf_out}" ]]
        then
            die "expected ELF output not found: ${elf_out}"
        fi

        "${objcopy}" -O binary "${elf_out}" "${efi_dir}/${efi_name}"
        step "Bootloader: ${efi_dir}/${efi_name} (ELF → flat binary)"
    else
        # x86_64: cargo produces a .efi PE directly.
        local cargo_out="${SERAPH_ROOT}/target/${boot_triple}/${OUTPUT_DIR}/seraph-boot.efi"
        if [[ -f "${cargo_out}" ]]
        then
            cp "${cargo_out}" "${efi_dir}/${efi_name}"
            step "Bootloader: ${efi_dir}/${efi_name}"
        fi
    fi
}

build_kernel()
{
    step "Building kernel for ${ARCH} (${PROFILE})"

    cargo build \
        --manifest-path "${SERAPH_ROOT}/kernel/Cargo.toml" \
        --bin seraph-kernel \
        --target "${KERNEL_TRIPLE}" \
        -Zbuild-std=core,alloc,compiler_builtins \
        -Zbuild-std-features=compiler-builtins-mem \
        ${CARGO_PROFILE_FLAG}

    local cargo_out="${SERAPH_ROOT}/target/${KERNEL_TRIPLE}/${OUTPUT_DIR}/seraph-kernel"
    if [[ -f "${cargo_out}" ]]
    then
        mkdir -p "${SYSROOT_EFI_SERAPH}"
        cp "${cargo_out}" "${SYSROOT_EFI_SERAPH}/seraph-kernel"
        step "Kernel: ${SYSROOT_EFI_SERAPH}/seraph-kernel"
    fi
}

build_init()
{
    step "Building init for ${ARCH} (${PROFILE})"

    cargo build \
        --manifest-path "${SERAPH_ROOT}/init/Cargo.toml" \
        --bin seraph-init \
        --target "${KERNEL_TRIPLE}" \
        -Zbuild-std=core,compiler_builtins \
        -Zbuild-std-features=compiler-builtins-mem \
        ${CARGO_PROFILE_FLAG}

    local cargo_out="${SERAPH_ROOT}/target/${KERNEL_TRIPLE}/${OUTPUT_DIR}/seraph-init"
    if [[ -f "${cargo_out}" ]]
    then
        mkdir -p "${SERAPH_SYSROOT}/sbin"
        cp "${cargo_out}" "${SERAPH_SYSROOT}/sbin/init"
        step "Init: ${SERAPH_SYSROOT}/sbin/init"
    fi
}

# install_config: copy all files from config/ into the sysroot, preserving
# their relative paths. Each file's destination directory is created as needed.
# To add a new config file, place it under config/ — no build script changes
# required.
install_config()
{
    local src_root="${SERAPH_ROOT}/config"
    local dst_root="${SERAPH_SYSROOT}"

    # Map config/ subdirectory names to sysroot destinations.
    # config/boot.conf -> sysroot/EFI/seraph/boot.conf
    # Add entries here as new config subtrees are introduced.
    declare -A DEST_MAP=(
        ["boot.conf"]="${SYSROOT_EFI_SERAPH}/boot.conf"
    )

    while IFS= read -r -d '' src_file
    do
        local rel="${src_file#"${src_root}/"}"
        local dst

        if [[ -v DEST_MAP["${rel}"] ]]
        then
            dst="${DEST_MAP["${rel}"]}"
        else
            # Default: mirror the config/ layout directly under sysroot/.
            dst="${dst_root}/${rel}"
        fi

        mkdir -p "$(dirname "${dst}")"
        cp "${src_file}" "${dst}"
        step "Config: ${dst}"
    done < <(find "${src_root}" -type f -print0 | sort -z)
}

# ── Dispatch ──────────────────────────────────────────────────────────────────

case "${COMPONENT}" in
    boot)
        build_boot
        ;;
    kernel)
        build_kernel
        ;;
    init)
        build_init
        ;;
    all)
        build_boot
        build_kernel
        build_init
        install_config
        ;;
    *)
        die "unknown component '${COMPONENT}' (supported: boot, kernel, init, all)"
        ;;
esac

record_sysroot_arch "${ARCH}"
step "Build complete (${ARCH}, ${PROFILE})"
