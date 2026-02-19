#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Copyright (C) 2026 George Kottler <mail@kottlerg.com>

# run.sh

# Build and run Seraph under QEMU.
#
# Usage:
#   ./run.sh [OPTIONS]
#
# Options:
#   --arch ARCH     Target architecture: x86_64 (default), riscv64
#   --release       Use the release build
#   --no-build      Skip the build step (use existing sysroot artifacts)
#   --gdb           Start QEMU with a GDB server on localhost:1234 (-s -S)
#   --riscv-edk2-verbose  Show riscv64 edk2 DEBUG output (filtered by default; no effect on x86_64)
#   -h, --help      Show this help and exit
#
# Requirements (see scripts/README.md for per-distro install commands):
#   x86_64:  OVMF UEFI firmware, qemu-system-x86
#   riscv64: edk2 RISC-V UEFI firmware, qemu-system-riscv
#
# The sysroot is used directly as the QEMU virtual FAT drive. OVMF finds the
# bootloader at EFI/BOOT/BOOTX64.EFI; the kernel is at boot/seraph-kernel.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/env.sh
source "${SCRIPT_DIR}/scripts/env.sh"

ARCH="${SERAPH_ARCH}"
CARGO_PROFILE_FLAG=""
NO_BUILD=false
GDB=false
RISCV_EDK2_VERBOSE=false

while [[ $# -gt 0 ]]
do
    case "$1" in
        --arch)
            ARCH="$2"; shift 2
            ;;
        --release)
            CARGO_PROFILE_FLAG="--release"
            shift
            ;;
        --no-build)
            NO_BUILD=true; shift
            ;;
        --gdb)
            GDB=true; shift
            ;;
        --riscv-edk2-verbose)
            RISCV_EDK2_VERBOSE=true; shift
            ;;
        -h|--help)
            sed -n '7,25p' "$0"
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

validate_arch "${ARCH}"

if [[ "${NO_BUILD}" == false ]]
then
    "${SCRIPT_DIR}/build.sh" --arch "${ARCH}" ${CARGO_PROFILE_FLAG}
fi

GDB_FLAGS=""
if [[ "${GDB}" == true ]]
then
    GDB_FLAGS="-s -S"
    step "GDB server will listen on localhost:1234 (QEMU paused until debugger connects)"
fi

# Restore terminal state when QEMU exits. OVMF sends xterm window-resize
# sequences (ESC[8;rows;colst) to the serial port during boot, which cause the
# terminal emulator to resize its window. Capture the original dimensions now
# and send the resize sequence back on exit so the terminal returns to normal.
_ORIG_SIZE="$(stty size 2>/dev/null || echo '24 80')"
_ORIG_ROWS="${_ORIG_SIZE% *}"
_ORIG_COLS="${_ORIG_SIZE#* }"
trap 'stty sane 2>/dev/null; printf "\033[8;%s;%st\033[?25h" "${_ORIG_ROWS}" "${_ORIG_COLS}" 2>/dev/null || true' EXIT

# ── Launch QEMU ───────────────────────────────────────────────────────────────

case "${ARCH}" in
    x86_64)
        EFI_NAME="$(boot_efi_filename "${ARCH}")"
        BOOT_EFI="${SERAPH_SYSROOT}/EFI/BOOT/${EFI_NAME}"
        KERNEL_BIN="${SERAPH_SYSROOT}/EFI/seraph/seraph-kernel"

        if [[ ! -f "${BOOT_EFI}" ]]
        then
            die "bootloader not found: ${BOOT_EFI}"
        fi
        if [[ ! -f "${KERNEL_BIN}" ]]
        then
            die "kernel not found: ${KERNEL_BIN}"
        fi

        # Locate OVMF firmware. UEFI firmware is required — SeaBIOS cannot
        # load UEFI applications. See scripts/README.md for install instructions.
        OVMF_CODE=""
        for path in \
            /usr/share/edk2/ovmf/OVMF_CODE.fd \
            /usr/share/OVMF/OVMF_CODE.fd \
            /usr/share/edk2-ovmf/x64/OVMF_CODE.fd \
            /usr/share/ovmf/OVMF.fd
        do
            if [[ -f "${path}" ]]
            then
                OVMF_CODE="${path}"
                break
            fi
        done

        if [[ -z "${OVMF_CODE}" ]]
        then
            die "OVMF firmware not found (package: edk2-ovmf / ovmf — see scripts/README.md)"
        fi

        step "Starting QEMU (x86_64, KVM, UEFI)"

        # The sysroot is passed directly as the virtual FAT drive. OVMF will
        # find the bootloader at EFI/BOOT/BOOTX64.EFI within the sysroot.
        # shellcheck disable=SC2086
        qemu-system-x86_64 \
            -machine q35 \
            -enable-kvm \
            -cpu host \
            -m 512M \
            -smp 4 \
            -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}" \
            -drive "format=raw,file=fat:rw:${SERAPH_SYSROOT}" \
            -serial stdio \
            -no-reboot \
            -no-shutdown \
            ${GDB_FLAGS}
        ;;

    riscv64)
        EFI_NAME="$(boot_efi_filename "${ARCH}")"
        BOOT_EFI="${SERAPH_SYSROOT}/EFI/BOOT/${EFI_NAME}"
        KERNEL_BIN="${SERAPH_SYSROOT}/EFI/seraph/seraph-kernel"

        if [[ ! -f "${BOOT_EFI}" ]]
        then
            die "bootloader not found: ${BOOT_EFI}"
        fi
        if [[ ! -f "${KERNEL_BIN}" ]]
        then
            die "kernel not found: ${KERNEL_BIN}"
        fi

        # Locate edk2 RISC-V firmware. See scripts/README.md for install instructions.
        RISCV_CODE=""
        RISCV_VARS_TEMPLATE=""
        for dir in \
            /usr/share/edk2/riscv \
            /usr/share/edk2-riscv \
            /usr/share/qemu-efi-riscv64
        do
            if [[ -f "${dir}/RISCV_VIRT_CODE.fd" ]]
            then
                RISCV_CODE="${dir}/RISCV_VIRT_CODE.fd"
                RISCV_VARS_TEMPLATE="${dir}/RISCV_VIRT_VARS.fd"
                break
            fi
        done

        if [[ -z "${RISCV_CODE}" ]]
        then
            die "edk2 RISC-V firmware not found (package: edk2-riscv64 / qemu-efi-riscv64 — see scripts/README.md)"
        fi
        if [[ ! -f "${RISCV_VARS_TEMPLATE}" ]]
        then
            die "RISC-V NVRAM template not found: ${RISCV_VARS_TEMPLATE}"
        fi

        # QEMU virt (>=9.0) requires pflash images to be exactly 32 MiB. Some
        # distro packages ship smaller .fd files. Pad to 32 MiB in temporary
        # copies so the originals are never modified.
        PFLASH_SIZE=$((32 * 1024 * 1024))

        RISCV_CODE_PADDED="$(mktemp --suffix=.fd)"
        cp "${RISCV_CODE}" "${RISCV_CODE_PADDED}"
        if [[ "$(wc -c < "${RISCV_CODE_PADDED}")" -lt "${PFLASH_SIZE}" ]]
        then
            truncate -s "${PFLASH_SIZE}" "${RISCV_CODE_PADDED}"
        fi

        # The VARS pflash drive must be writable (UEFI stores boot variables
        # there). Use a fresh temp copy each run for a reproducible UEFI state.
        RISCV_VARS="$(mktemp --suffix=.fd)"
        cp "${RISCV_VARS_TEMPLATE}" "${RISCV_VARS}"
        if [[ "$(wc -c < "${RISCV_VARS}")" -lt "${PFLASH_SIZE}" ]]
        then
            truncate -s "${PFLASH_SIZE}" "${RISCV_VARS}"
        fi

        # Extend the EXIT trap to delete both temp files after QEMU exits.
        trap 'rm -f "${RISCV_CODE_PADDED}" "${RISCV_VARS}"; stty sane 2>/dev/null; \
              printf "\033[8;%s;%st\033[?25h" "${_ORIG_ROWS}" "${_ORIG_COLS}" \
              2>/dev/null || true' EXIT

        # QEMU virt machine loads OpenSBI automatically; no explicit flag needed.
        # Two pflash drives: CODE (read-only firmware) and VARS (writable NVRAM).
        QEMU_ARGS=(
            -machine virt
            -m 512M
            -smp 4
            -drive "if=pflash,format=raw,readonly=on,file=${RISCV_CODE_PADDED}"
            -drive "if=pflash,format=raw,file=${RISCV_VARS}"
            -drive "format=raw,file=fat:rw:${SERAPH_SYSROOT}"
            -device virtio-gpu-pci
            -device qemu-xhci
            -device usb-kbd
            -device usb-tablet
            -serial stdio
            -no-reboot
            -no-shutdown
        )

        if [[ "${RISCV_EDK2_VERBOSE}" == false ]]
        then
            # The packaged edk2-riscv64 is typically a DEBUG build that produces
            # thousands of lines of diagnostic output before our bootloader loads.
            # Suppress it
            # by piping through awk until edk2 reports finding our EFI binary.
            # Piping QEMU's stdout disables the QEMU monitor (Ctrl+A c); serial
            # I/O to/from the bootloader and kernel is unaffected.
            step "Starting QEMU (riscv64, TCG, UEFI) [edk2 DEBUG output filtered; --riscv-edk2-verbose to disable]"
            # shellcheck disable=SC2086
            qemu-system-riscv64 \
                "${QEMU_ARGS[@]}" \
                ${GDB_FLAGS} \
                | awk '/BOOTRISCV64\.EFI/{show=1} show{print; fflush()}'
        else
            step "Starting QEMU (riscv64, TCG, UEFI)"
            # shellcheck disable=SC2086
            qemu-system-riscv64 \
                "${QEMU_ARGS[@]}" \
                ${GDB_FLAGS}
        fi
        ;;
esac
