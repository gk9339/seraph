#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Copyright (C) 2026 George Kottler <mail@kottlerg.com>

# test.sh

# Run Seraph unit tests on the host target.
#
# Tests compile for the host (not a bare-metal target), so no --arch flag is
# needed. The .cargo/config.toml deliberately omits -Zbuild-std from host
# builds; the test profile overrides panic=abort so the harness can catch panics.
#
# Usage:
#   ./test.sh [OPTIONS]
#
# Options:
#   --component COMPONENT  Test a single crate: boot, protocol, kernel, init, all (default: all)
#   -h, --help             Show this help and exit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/env.sh
source "${SCRIPT_DIR}/scripts/env.sh"

COMPONENT="all"

while [[ $# -gt 0 ]]
do
    case "$1" in
        --component)
            COMPONENT="$2"; shift 2
            ;;
        -h|--help)
            sed -n '7,18p' "$0"
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

# ── Component test functions ──────────────────────────────────────────────────

test_boot()
{
    step "Testing bootloader (host)"
    cargo test --manifest-path "${SERAPH_ROOT}/boot/loader/Cargo.toml"
}

test_protocol()
{
    step "Testing boot protocol (host)"
    cargo test --manifest-path "${SERAPH_ROOT}/boot/protocol/Cargo.toml"
}

test_kernel()
{
    step "Testing kernel (host)"
    cargo test --manifest-path "${SERAPH_ROOT}/kernel/Cargo.toml"
}

test_init()
{
    step "Testing init (host)"
    cargo test --manifest-path "${SERAPH_ROOT}/init/Cargo.toml"
}

# ── Dispatch ──────────────────────────────────────────────────────────────────

case "${COMPONENT}" in
    boot)
        test_boot
        ;;
    protocol)
        test_protocol
        ;;
    kernel)
        test_kernel
        ;;
    init)
        test_init
        ;;
    all)
        cargo test --workspace
        ;;
    *)
        die "unknown component '${COMPONENT}' (supported: boot, protocol, kernel, init, all)"
        ;;
esac

step "Tests complete"
