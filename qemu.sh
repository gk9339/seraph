#!/bin/sh
set -e
. ./mkiso.sh

qemu-system-$(./script/target-triplet-to-arch.sh $HOST) -serial stdio -cdrom seraph.iso
