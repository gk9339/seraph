# fs

Filesystem driver implementations. Each subdirectory is a separate binary
(FAT, ext4, tmpfs, etc.) launched and managed by `vfsd`.

## Relationship to other components

- `vfsd` — virtual filesystem server that routes namespace operations to the
  appropriate fs driver via IPC.
- `drivers/` — hardware device drivers (block devices, etc.) managed by devmgr.
  fs drivers communicate with block device drivers via IPC; they do not access
  hardware directly.

## Adding a filesystem

Create a new subdirectory with its own `Cargo.toml`. The binary receives IPC
messages from vfsd conforming to the fs driver protocol (TBD).

## Status

Not yet implemented.

---

## Summarized By

None
