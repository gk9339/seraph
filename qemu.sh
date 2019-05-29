#!/bin/sh
set -e
. ./mkiso.sh

qemu-system-$(./script/target-triplet-to-arch.sh $HOST) -cdrom seraph.iso
