# vfsd

Virtual filesystem daemon. Provides a unified namespace over multiple underlying
filesystem drivers (see `fs/`), each running as a separate process and communicating
with vfsd via IPC. Block device access goes through the appropriate driver endpoint,
received from devmgr after hardware enumeration.

vfsd exposes a filesystem IPC interface to applications and other services. It does
not touch hardware directly — all storage access is mediated through driver IPC.

## Filesystem drivers

Per-filesystem implementations (FAT, ext4, tmpfs, etc.) live in `fs/` as separate
binaries. vfsd launches and manages them, routing namespace operations to the
correct driver. This keeps vfsd itself small and filesystem implementations isolated.

---

## Summarized By

None
