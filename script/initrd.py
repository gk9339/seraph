#!/usr/bin/python3

import tarfile

restricted_files = {
        'bin/init' : 0o700,
        }

def file_filter(tarinfo):
    tarinfo.uid = 0
    tarinfo.gid = 0
    tarinfo.uname = "root"
    tarinfo.gname = "root"

    if tarinfo.name in restricted_files:
        tarinfo.mode = restricted_files[tarinfo.name]

    return tarinfo

with tarfile.open('./sysroot/boot/seraph.initrd', 'w') as ramdisk:
    ramdisk.add('sysroot', arcname='/', filter=file_filter)
