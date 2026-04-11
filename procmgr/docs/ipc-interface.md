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

Create a new process from a raw ELF module.

**Request:**

| Field | Value |
|---|---|
| label | 1 |
| cap[0] | Frame capability for the ELF module image |

The caller transfers a Frame cap covering the raw ELF bytes. procmgr maps
the frame, parses the ELF, creates an address space, CSpace, and thread,
maps LOAD segments, populates the `ProcessInfo` handover page, and starts
the thread.

**Reply (success):**

| Field | Value |
|---|---|
| label | 0 (success) |
| data[0] | Process ID (procmgr-assigned identifier) |

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

### Label 2: `EXIT_PROCESS`

Deferred. Not implemented in Tier 1.

### Label 3: `QUERY_PROCESS`

Deferred. Not implemented in Tier 1.

---

## Capability Transfer

Capability transfer uses the IPC message's cap slot array (up to 4 caps per
message). On `CREATE_PROCESS`, the caller's Frame cap is moved into procmgr's
CSpace atomically with the message delivery. procmgr consumes the cap during
process creation and does not return it.

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
