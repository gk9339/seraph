#!/bin/bash
set -e
. ./mkiso.sh

qemu-system-$(./script/target-triplet-to-arch.sh $HOST) -m 512M -serial stdio -cdrom seraph.iso
