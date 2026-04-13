# Namespace

Mount point management and path resolution for the virtual filesystem namespace.

---

## Mount Table

vfsd maintains a fixed-size mount table mapping path prefixes to filesystem
driver endpoints. Maximum 4 mount entries.

Each entry contains:

| Field | Type | Description |
|---|---|---|
| `path` | `[u8; 64]` | Null-terminated mount path prefix (e.g., `/`, `/mnt/data`) |
| `path_len` | `usize` | Length of the path prefix in bytes |
| `driver_ep` | `u32` | Send-side capability to the filesystem driver's IPC endpoint |
| `active` | `bool` | Whether this entry is in use |

Mount entries are added during vfsd startup (from the startup message) and
potentially via future mount operations. Entries are removed on unmount or
driver crash.

---

## Path Resolution

Given an absolute path, vfsd resolves it to a mount entry using longest-prefix
matching:

1. Iterate all active mount entries.
2. For each entry, check whether the path starts with the entry's prefix.
3. Select the entry with the longest matching prefix.
4. Strip the matched prefix from the path to produce the driver-relative path.

For a mount at `/`, all paths match and the full path (minus the leading `/`)
is forwarded to the driver.

Case sensitivity depends on the underlying filesystem. vfsd performs
case-sensitive prefix matching on the mount path; case-insensitive matching
is the filesystem driver's responsibility.

---

## Summarized By

None
