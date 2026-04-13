# VFS IPC Interface

IPC interface exposed by vfsd to applications and services: open, read, close,
stat, and readdir operations on the unified filesystem namespace.

---

## Endpoint

vfsd listens on a single IPC endpoint. Init creates the endpoint during
bootstrap and injects the Receive-side capability into vfsd's CSpace. Send-side
capabilities are delegated to any process that needs filesystem access.

---

## Messages

All requests use `SYS_IPC_CALL` (synchronous call/reply). The message label
field identifies the operation. Data words and capability slots carry arguments;
the reply carries results.

### Label 1: `OPEN`

Open a file or directory by path. Returns a file descriptor for subsequent
operations.

**Request:**

| Field | Value |
|---|---|
| label | `1 \| (path_len << 16)` — bits 0–15 = opcode, bits 16–31 = path byte count |
| data[0..5] | Path bytes packed into up to 6 data words (48 bytes max) |

Path is an absolute, `/`-separated, null-free byte string. Case-insensitive
matching is performed for FAT mount points. Maximum path length is 48 bytes;
longer paths are not supported in this version.

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | File descriptor (u64, vfsd-assigned) |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 1 | `NotFound` | Path does not resolve to an existing entry |
| 2 | `NoMount` | No filesystem mounted at the resolved prefix |
| 3 | `TooManyOpen` | File descriptor table is full |

### Label 2: `READ`

Read bytes from an open file at a given offset.

**Request:**

| Field | Value |
|---|---|
| label | 2 |
| data[0] | File descriptor |
| data[1] | Byte offset into the file |
| data[2] | Maximum bytes to read (capped at 512) |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Bytes actually read (may be less than requested at EOF) |
| data[1..] | File data packed into data words |

Up to 512 bytes of file data are returned in the IPC buffer (64 u64 words in
the extended payload region). The caller reads `data[0]` bytes starting from
`data[1]`.

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 4 | `InvalidFd` | File descriptor is not open or out of range |
| 5 | `IoError` | Underlying driver returned an error |

### Label 3: `CLOSE`

Close an open file descriptor and release associated resources.

**Request:**

| Field | Value |
|---|---|
| label | 3 |
| data[0] | File descriptor |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 4 | `InvalidFd` | File descriptor is not open or out of range |

### Label 4: `STAT`

Query metadata for an open file descriptor.

**Request:**

| Field | Value |
|---|---|
| label | 4 |
| data[0] | File descriptor |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | File size in bytes |
| data[1] | Flags: bit 0 = directory, bit 1 = read-only |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 4 | `InvalidFd` | File descriptor is not open or out of range |

### Label 5: `READDIR`

Read a directory entry by index from an open directory descriptor.

**Request:**

| Field | Value |
|---|---|
| label | 5 |
| data[0] | File descriptor (must refer to a directory) |
| data[1] | Entry index (0-based) |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Entry name length in bytes |
| data[1] | File size (0 for directories) |
| data[2] | Flags: bit 0 = directory |
| data[3..] | Entry name bytes (8.3 format, up to 12 bytes) |

**Reply (end of directory):**

| Field | Value |
|---|---|
| label | 6 (`EndOfDir`) |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 4 | `InvalidFd` | File descriptor is not open or not a directory |

---

## File Descriptors

vfsd maintains a global file descriptor table. Each descriptor maps to a
(mount index, driver-side fd) pair. The table has a fixed capacity of 16
entries. Descriptors are allocated on `OPEN` and freed on `CLOSE`.

File descriptors are opaque integers. Clients MUST NOT assume any relationship
between descriptor values and internal state.

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/ipc-design.md](../../docs/ipc-design.md) | IPC message format, cap transfer protocol |
| [fs/docs/fs-driver-protocol.md](../../fs/docs/fs-driver-protocol.md) | vfsd-to-driver IPC |
| [docs/architecture.md](../../docs/architecture.md) | vfsd role in the system |

---

## Summarized By

None
