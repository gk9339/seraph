# vfs

Virtual filesystem server. Provides a unified namespace over multiple underlying
filesystems (ext2, FAT, etc.), each running as a separate process and communicating
with vfs via IPC. Block device access goes through the appropriate driver endpoint,
received from devmgr after hardware enumeration.

vfs exposes a filesystem IPC interface to applications and other services. It does
not touch hardware directly — all storage access is mediated through driver IPC.
