#!/bin/sh
set -e
. ./mkiso.sh

qemu-system-$(./sh/target-triplet-to-arch.sh $HOST) -cdrom seraph.iso
