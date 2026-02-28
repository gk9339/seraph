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
#   --headless      Run without a display window (-display none)
#   --verbose       Show all serial output including pre-boot firmware noise (filtered by default)
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
HEADLESS=false
VERBOSE=false

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
        --headless)
            HEADLESS=true; shift
            ;;
        --verbose)
            VERBOSE=true; shift
            ;;
        -h|--help)
            sed -n '7,27p' "$0"
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
GDB_ACCEL_FLAGS=""
if [[ "${GDB}" == true ]]
then
    GDB_FLAGS="-s -S"
    # KVM prevents QEMU's gdbserver from reading live CPU register state —
    # all vCPUs appear frozen at the reset vector (0xfff0) regardless of
    # actual execution position. Disable KVM and fall back to TCG when
    # debugging so register reads and breakpoints work correctly.
    # TCG is ~5-10x slower than KVM; OVMF may take ~30s to reach the
    # bootloader instead of ~5s. That is acceptable for a debug session.
    GDB_ACCEL_FLAGS="-accel tcg -cpu qemu64"
    step "GDB server will listen on localhost:1234 (QEMU paused; KVM disabled for correct register visibility)"
fi

# Restore terminal state when QEMU exits. OVMF sends xterm window-resize
# sequences (ESC[8;rows;colst) to the serial port during boot, which cause the
# terminal emulator to resize its window. Capture the original dimensions now
# and send the resize sequence back on exit so the terminal returns to normal.
_ORIG_SIZE="$(stty size 2>/dev/null || echo '24 80')"
_ORIG_ROWS="${_ORIG_SIZE% *}"
_ORIG_COLS="${_ORIG_SIZE#* }"
trap 'stty sane 2>/dev/null; printf "\033[8;%s;%st\033[?25h" "${_ORIG_ROWS}" "${_ORIG_COLS}" 2>/dev/null || true' EXIT

# ── Validate bootloader and kernel files ──────────────────────────────────────

EFI_NAME="$(boot_efi_filename "${ARCH}")"
BOOT_EFI="${SERAPH_SYSROOT}/EFI/BOOT/${EFI_NAME}"
KERNEL_BIN="${SERAPH_SYSROOT}/EFI/seraph/seraph-kernel"

[[ -f "${BOOT_EFI}" ]] || die "bootloader not found: ${BOOT_EFI}"
[[ -f "${KERNEL_BIN}" ]] || die "kernel not found: ${KERNEL_BIN}"
INIT_BIN="${SERAPH_SYSROOT}/sbin/init"
[[ -f "${INIT_BIN}" ]] || die "init not found: ${INIT_BIN}"

# ── Build common QEMU args ────────────────────────────────────────────────────

QEMU_ARGS=(
    -m 512M
    -smp 1
    -drive "format=raw,file=fat:rw:${SERAPH_SYSROOT}"
    -serial stdio
    -no-reboot
    -no-shutdown
)

if [[ "${HEADLESS}" == true ]]
then
    QEMU_ARGS+=(-display none)
fi

if [[ -n "${GDB_FLAGS}" ]]
then
    # shellcheck disable=SC2206
    QEMU_ARGS+=(${GDB_FLAGS})
fi

# ── Launch QEMU ───────────────────────────────────────────────────────────────

case "${ARCH}" in
    x86_64)
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

        QEMU_ARGS+=(
            -machine q35
            ${GDB_ACCEL_FLAGS:--enable-kvm -cpu host}
            -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
        )

        if [[ "${HEADLESS}" == true ]]
        then
            QEMU_ARGS+=(-vga none)
        fi

        QEMU_BIN="qemu-system-x86_64"
        QEMU_DESC="x86_64, UEFI"
        ;;

    riscv64)
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

        QEMU_ARGS+=(
            -machine virt
            -drive "if=pflash,format=raw,readonly=on,file=${RISCV_CODE_PADDED}"
            -drive "if=pflash,format=raw,file=${RISCV_VARS}"
        )

        if [[ "${HEADLESS}" != true ]]
        then
            QEMU_ARGS+=(
                -device ramfb
                -device qemu-xhci
                -device usb-kbd
            )
        fi

        # QEMU virt machine loads OpenSBI automatically; no explicit flag needed.
        QEMU_BIN="qemu-system-riscv64"
        QEMU_DESC="riscv64, TCG, UEFI"
        ;;
esac

# Apply serial output filter unless --verbose is set. Filters all output until
# the first line containing 'seraph-boot', suppressing pre-boot firmware noise
# (UEFI DEBUG spam, OpenSBI banners, etc.) that is irrelevant to normal runs.
# Piping QEMU's stdout disables the QEMU monitor (Ctrl+A c); serial I/O to/from
# the bootloader and kernel is unaffected.
if [[ "${VERBOSE}" == false ]]
then
    step "Starting QEMU (${QEMU_DESC}) [output filtered until 'seraph-boot'; --verbose to disable]"
    "${QEMU_BIN}" "${QEMU_ARGS[@]}" \
        | awk '/seraph-boot/{show=1} show{print; fflush()}'
else
    step "Starting QEMU (${QEMU_DESC})"
    "${QEMU_BIN}" "${QEMU_ARGS[@]}"
fi
