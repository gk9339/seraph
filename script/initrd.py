#!/usr/bin/python3

import tarfile

print("generating initrd.");

def file_filter(tarinfo):
    tarinfo.uid = 0;
    tarinfo.gid = 0;

    return tarinfo

with tarfile.open('./sysroot/boot/seraph.initrd', 'w') as ramdisk:
    ramdisk.add('sysroot', arcname='/', filter=file_filter)
