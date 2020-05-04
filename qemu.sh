#!/bin/bash
set -e
. ./mkiso.sh

echo -e "    \033[1m\033[38;5;14m:: Launching QEMU\033[0m"

echo "qemu-system-$(./script/target-triplet-to-arch.sh $HOST) -m 512M -serial stdio -rtc base=localtime -cdrom seraph.iso"
qemu-system-$(./script/target-triplet-to-arch.sh $HOST) -m 512M -serial stdio -rtc base=localtime -cdrom seraph.iso
