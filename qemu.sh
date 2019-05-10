#!/bin/sh
set -e
. ./mkiso.sh

qemu-system-$(./target-triplet-to-arch.sh $HOST) -cdrom seraph.iso
