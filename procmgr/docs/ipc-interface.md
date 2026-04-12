# procmgr IPC Interface

IPC interface specification for procmgr: message labels, capability transfer
semantics, and error conditions for process lifecycle operations.

---

## Endpoint

procmgr listens on a single IPC endpoint. Init holds the Send-side capability
and passes it (or a derived copy) to any service that needs to create processes.
procmgr holds the Receive-side capability.

---

## Messages

All requests use `SYS_IPC_CALL` (synchronous call/reply). The message label
field identifies the operation. Data words and capability slots carry arguments;
the reply carries results.

### Label 1: `CREATE_PROCESS`

Create a new process from a raw ELF module. The process is created in a
**suspended** state — the thread is not started. The caller receives the
child's `CSpace` capability and `ProcessInfo` frame capability so it can
inject initial capabilities and write `CapDescriptor` / startup message data
before starting the process via `START_PROCESS`.

**Request:**

| Field | Value |
|---|---|
| label | 1 |
| cap[0] | Frame capability for the ELF module image |

The caller transfers a Frame cap covering the raw ELF bytes. procmgr maps
the frame, parses the ELF, creates an address space, CSpace, and thread,
maps LOAD segments, and populates the `ProcessInfo` handover page with
identity caps. The thread is **not** started.

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Process ID (procmgr-assigned identifier) |
| cap[0] | Child `CSpace` capability (full rights) |
| cap[1] | `ProcessInfo` frame capability (MAP\|WRITE rights) |

The `CSpace` cap allows the caller to inject capabilities into the child's
capability space via `cap_copy`. The `ProcessInfo` frame cap allows the
caller to map the page writable and patch `initial_caps_base`,
`initial_caps_count`, `cap_descriptor_count`, `cap_descriptors_offset`,
and startup message fields.

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |
| data[0] | 0 |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 1 | `InvalidElf` | ELF validation failed (bad magic, wrong arch, corrupt headers) |
| 2 | `OutOfMemory` | Insufficient frame caps to allocate stack, ProcessInfo page, or IPC buffer |
| 3 | `CSpaceFull` | Cannot allocate kernel objects (address space, CSpace, thread) |

### Label 2: `START_PROCESS`

Start a previously created (suspended) process. The caller must have
completed any capability injection and `ProcessInfo` patching before
calling this operation.

**Request:**

| Field | Value |
|---|---|
| label | 2 |
| data[0] | Process ID (from `CREATE_PROCESS` reply) |

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
| 4 | `InvalidPid` | No process with the given PID exists |
| 5 | `AlreadyStarted` | Process was already started |

### Label 3: `EXIT_PROCESS`

Deferred. Not implemented.

### Label 4: `QUERY_PROCESS`

Deferred. Not implemented.

### Label 5: `REQUEST_FRAMES`

Allocate page-sized Frame capabilities from the procmgr memory pool and
transfer them to the caller. Intended for drivers that need DMA-capable
memory at runtime (virtqueue structures, data buffers).

**Request:**

| Field | Value |
|---|---|
| label | 5 |
| data[0] | Number of pages to allocate (1–4) |

The caller specifies how many 4 KiB pages to allocate. Maximum 4 pages per
request (limited by the IPC cap slot count). The caller may issue multiple
requests for larger allocations.

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Number of pages actually allocated |
| cap[0..N] | Frame capabilities (one per page, MAP\|WRITE rights) |

**Reply (error):**

| Field | Value |
|---|---|
| label | Nonzero error code |

**Error codes:**

| Code | Name | Meaning |
|---|---|---|
| 6 | `OutOfMemory` | Insufficient frame caps in the memory pool |
| 7 | `InvalidArgument` | Requested page count is 0 or exceeds 4 |

---

## Capability Transfer

Capability transfer uses the IPC message's cap slot array (up to 4 caps per
message). On `CREATE_PROCESS`, the caller's Frame cap is moved into procmgr's
CSpace atomically with the message delivery. procmgr consumes the cap during
process creation and does not return it.

On reply, procmgr transfers derived copies of the child's `CSpace` and
`ProcessInfo` frame capabilities to the caller. procmgr retains the original
caps for process lifecycle management.

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/ipc-design.md](../../docs/ipc-design.md) | IPC message format, cap transfer protocol |
| [abi/process-abi](../../abi/process-abi/README.md) | ProcessInfo handover struct |
| [abi/syscall](../../abi/syscall/README.md) | Syscall numbers and register conventions |

---

## Summarized By

[procmgr/README.md](../README.md)
