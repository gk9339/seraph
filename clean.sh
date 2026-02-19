#!/usr/bin/env bash
# clean.sh -- Remove Seraph build artifacts.
#
# Usage:
#   ./clean.sh [OPTIONS]
#
# Options:
#   --all       Also remove the cargo target/ directory (full clean)
#   -h, --help  Show this help and exit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/env.sh
source "${SCRIPT_DIR}/scripts/env.sh"

FULL_CLEAN=false

while [[ $# -gt 0 ]]
do
    case "$1" in
        --all)
            FULL_CLEAN=true; shift
            ;;
        -h|--help)
            sed -n '2,9p' "$0"
            exit 0
            ;;
        *)
            die "unknown option: $1"
            ;;
    esac
done

if [[ -d "${SERAPH_SYSROOT}" ]]
then
    step "Removing sysroot: ${SERAPH_SYSROOT}"
    rm -rf "${SERAPH_SYSROOT}"
else
    step "Sysroot already clean"
fi

if [[ "${FULL_CLEAN}" == true ]]
then
    step "Removing cargo target/ directory"
    cargo clean --manifest-path "${SERAPH_ROOT}/Cargo.toml"
fi

step "Clean complete"
