# rootfs

Source files that get installed into the sysroot during builds. These form part
of the root filesystem image served to the virtual machine.

The `build.sh` `install_rootfs()` function copies every file here into the
sysroot, mapping filenames to their final destinations via `DEST_MAP`. To add a
new file, place it in this directory; update `DEST_MAP` in `build.sh` if the
destination path differs from a direct mirror of this layout.

## Current contents

| File | Sysroot destination |
|---|---|
| `boot.conf` | `EFI/seraph/boot.conf` |
