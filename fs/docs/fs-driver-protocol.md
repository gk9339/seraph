# Filesystem Driver Protocol

IPC protocol between vfsd and filesystem drivers. vfsd creates a per-driver
endpoint and sends operations on it; the driver receives and replies.

---

## Endpoint

vfsd creates a dedicated IPC endpoint for each filesystem driver at mount time.
vfsd holds the Send-side capability; the driver holds the Receive-side
capability, injected into the driver's CSpace during two-phase process creation.

---

## Messages

All operations use `SYS_IPC_CALL` (synchronous call/reply). Labels mirror the
vfsd namespace interface where applicable. The driver processes one request at a
time (single-threaded service loop).

### Label 10: `FS_MOUNT`

Initialize the filesystem. Sent once after the driver starts. The driver reads
superblock/BPB metadata from the block device and prepares internal state.

The block device endpoint is injected into the driver's CSpace at creation
time (identified by `CapDescriptor` with `CapType::Frame` and
`aux0 = BLOCK_ENDPOINT_SENTINEL`). No capabilities are transferred in this
message.

**Request:**

| Field | Value |
|---|---|
| label | 10 |

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
| 1 | `InvalidFilesystem` | Superblock/BPB validation failed |
| 2 | `IoError` | Block device read failed |

### Label 1: `FS_OPEN`

Open a file or directory by path within this filesystem.

**Request:**

| Field | Value |
|---|---|
| label | `1 \| (path_len << 16)` — bits 0–15 = opcode, bits 16–31 = path byte count |
| data[0..5] | Path bytes (relative to this mount point, `/`-separated) |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Driver-assigned file descriptor |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 1 | `NotFound` | Path does not resolve to an existing entry |
| 3 | `TooManyOpen` | Driver fd table is full |

### Label 2: `FS_READ`

Read bytes from an open file at a given offset.

**Request:**

| Field | Value |
|---|---|
| label | 2 |
| data[0] | Driver file descriptor |
| data[1] | Byte offset |
| data[2] | Maximum bytes to read (capped at 512) |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Bytes actually read |
| data[1..] | File data in IPC buffer (up to 512 bytes, 64 u64 words) |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 4 | `InvalidFd` | Descriptor not open or out of range |
| 5 | `IoError` | Block device read failed |

### Label 3: `FS_CLOSE`

Close a driver-side file descriptor.

**Request:**

| Field | Value |
|---|---|
| label | 3 |
| data[0] | Driver file descriptor |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |

### Label 4: `FS_STAT`

Query metadata for an open file.

**Request:**

| Field | Value |
|---|---|
| label | 4 |
| data[0] | Driver file descriptor |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | File size in bytes |
| data[1] | Flags: bit 0 = directory, bit 1 = read-only |

### Label 5: `FS_READDIR`

Read a directory entry by index.

**Request:**

| Field | Value |
|---|---|
| label | 5 |
| data[0] | Driver file descriptor (must be a directory) |
| data[1] | Entry index (0-based) |

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Entry name length |
| data[1] | File size |
| data[2] | Flags: bit 0 = directory |
| data[3..] | Entry name bytes (8.3 format, up to 12 bytes) |

**Reply (end of directory):**

| Field | Value |
|---|---|
| label | 6 (`EndOfDir`) |

---

## Block Device Access

Filesystem drivers perform all storage I/O through a block device endpoint
received at creation time. The block device protocol uses label 1
(`READ_BLOCK`): data[0] = sector number, reply contains 512 bytes (64 u64
words) on success. See `drivers/virtio/blk` for the block device IPC
specification.

---

## Sentinel Values

Capabilities injected into a filesystem driver's CSpace are identified by
sentinel values in the `CapDescriptor.aux0` field:

| Sentinel | Meaning |
|---|---|
| `0xFFFF_FFFF_FFFF_FFFF` | Log endpoint |
| `0xFFFF_FFFF_FFFF_FFFE` | Service endpoint (vfsd-to-driver IPC, Receive-side) |
| `0xFFFF_FFFF_FFFF_FFFD` | Block device endpoint (Send-side) |
| `0x0000_0000_0000_0000` (aux0 and aux1 both 0) | procmgr endpoint |

All sentinels use `CapType::Frame` as the discriminant (the actual kernel
object is an Endpoint, but the CapDescriptor type field is overloaded for
sentinel identification).

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/ipc-design.md](../../docs/ipc-design.md) | IPC message format, cap transfer protocol |
| [vfsd/docs/vfs-ipc-interface.md](../../vfsd/docs/vfs-ipc-interface.md) | Client-facing namespace IPC |
| [docs/device-management.md](../../docs/device-management.md) | Block device endpoint origin |

---

## Summarized By

None
