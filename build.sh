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
#   --component COMPONENT Build only one component: boot, kernel, all (default: all)
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
KERNEL_JSON="$(kernel_target_json "${ARCH}")"

SYSROOT_EFI_SERAPH="${SERAPH_SYSROOT}/EFI/seraph"

# ── Component build functions ─────────────────────────────────────────────────

build_boot()
{
    local boot_triple boot_json efi_name
    boot_triple="$(boot_target_triple "${ARCH}")"
    boot_json="$(boot_target_json "${ARCH}")"
    efi_name="$(boot_efi_filename "${ARCH}")"

    step "Building bootloader for ${ARCH} (${PROFILE})"

    # x86_64 uses the built-in x86_64-unknown-uefi target; RISC-V needs a custom
    # target JSON and an extra linker script passed via RUSTFLAGS.
    local extra_cargo_flags=""
    local extra_rustflags=""

    if [[ -n "${boot_json}" ]]
    then
        if [[ ! -f "${boot_json}" ]]
        then
            die "boot target spec not found: ${boot_json}"
        fi
        extra_cargo_flags="-Zjson-target-spec"
        extra_rustflags="-C link-arg=-T${SERAPH_ROOT}/boot/loader/linker/riscv64-uefi.ld"
    fi

    # shellcheck disable=SC2086
    RUSTFLAGS="${extra_rustflags}" cargo build \
        --manifest-path "${SERAPH_ROOT}/boot/loader/Cargo.toml" \
        --target "${boot_json:-${boot_triple}}" \
        -Zbuild-std=core,compiler_builtins \
        -Zbuild-std-features=compiler-builtins-mem \
        ${extra_cargo_flags} \
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

    if [[ ! -f "${KERNEL_JSON}" ]]
    then
        die "kernel target spec not found: ${KERNEL_JSON}"
    fi

    cargo build \
        --manifest-path "${SERAPH_ROOT}/kernel/Cargo.toml" \
        --bin seraph-kernel \
        --target "${KERNEL_JSON}" \
        -Zbuild-std=core,alloc,compiler_builtins \
        -Zbuild-std-features=compiler-builtins-mem \
        -Zjson-target-spec \
        ${CARGO_PROFILE_FLAG}

    local cargo_out="${SERAPH_ROOT}/target/${KERNEL_TRIPLE}/${OUTPUT_DIR}/seraph-kernel"
    if [[ -f "${cargo_out}" ]]
    then
        mkdir -p "${SYSROOT_EFI_SERAPH}"
        cp "${cargo_out}" "${SYSROOT_EFI_SERAPH}/seraph-kernel"
        step "Kernel: ${SYSROOT_EFI_SERAPH}/seraph-kernel"
    fi
}

# ── Dispatch ──────────────────────────────────────────────────────────────────

case "${COMPONENT}" in
    boot)
        build_boot
        ;;
    kernel)
        build_kernel
        ;;
    all)
        build_boot
        build_kernel
        ;;
    *)
        die "unknown component '${COMPONENT}' (supported: boot, kernel, all)"
        ;;
esac

record_sysroot_arch "${ARCH}"
step "Build complete (${ARCH}, ${PROFILE})"
