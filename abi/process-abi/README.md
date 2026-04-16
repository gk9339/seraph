# abi/process-abi

Userspace process startup ABI: the binary contract between a process creator
and the created process, plus the universal `main()` entry point convention for
all Seraph userspace programs.

---

## Overview

Process startup in Seraph has two layers:

1. **Handover struct** — a `#[repr(C)]`, version-gated structure placed at a
   well-known virtual address before the new process runs. The producer is
   procmgr for all normal processes. The struct provides the process with its
   initial capability layout, IPC buffer, and startup context.

2. **`main()` convention** — the universal entry point signature. Every Seraph
   userspace binary defines `main()` with a `&StartupInfo` argument.
   `_start()` (provided by a runtime crate) reads the handover struct,
   constructs a `StartupInfo`, and calls `main()`.

Both layers are defined in this crate. Init is a special case: its handover
struct is `InitInfo` (from [`abi/init-protocol`](../init-protocol/README.md)),
populated by the kernel rather than procmgr. Init's `_start()` converts
`InitInfo` into `StartupInfo` before calling `main()`, so all processes —
including init and ktest — share the same `main()` signature.

---

## ProcessInfo

The procmgr-to-process handover struct. Placed by procmgr at
`PROCESS_INFO_VADDR` (a fixed virtual address in every new process's address
space) in a single read-only page, analogous to how the kernel places `InitInfo`
for init at `INIT_INFO_VADDR`.

The struct MUST be `#[repr(C)]` with stable layout. The process MUST check
`version == PROCESS_ABI_VERSION` before accessing any other field.

```rust
#[repr(C)]
pub struct ProcessInfo {
    /// Protocol version. Must equal `PROCESS_ABI_VERSION`.
    pub version: u32,

    // ── Process identity ────────────────────────────────────────────

    /// CSpace slot of the process's own Thread capability (Control right).
    pub self_thread_cap: u32,

    /// CSpace slot of the process's own AddressSpace capability.
    pub self_aspace_cap: u32,

    /// CSpace slot of the process's own CSpace capability.
    pub self_cspace_cap: u32,

    // ── IPC ─────────────────────────────────────────────────────────

    /// Virtual address of the pre-mapped IPC buffer page.
    ///
    /// Every thread requires a registered IPC buffer for extended message
    /// payloads. procmgr maps this page and registers it with the kernel
    /// before the process starts.
    pub ipc_buffer_vaddr: u64,

    /// CSpace slot of an IPC endpoint back to the creating service.
    ///
    /// For processes created by procmgr directly, this is an endpoint to
    /// procmgr. For processes created on behalf of another service (e.g.
    /// devmgr requesting a driver), the endpoint MAY point to the
    /// requesting service instead. Zero if no creator endpoint is provided.
    pub creator_endpoint_cap: u32,

    // ── Initial capabilities ────────────────────────────────────────

    /// First CSpace slot containing service-specific initial capabilities.
    ///
    /// The capabilities in slots `initial_caps_base` through
    /// `initial_caps_base + initial_caps_count - 1` are the process's
    /// initial authority, delegated by the creating service.
    pub initial_caps_base: u32,

    /// Number of initial capability slots.
    pub initial_caps_count: u32,

    /// Number of `CapDescriptor` entries following this struct.
    pub cap_descriptor_count: u32,

    /// Byte offset from the start of this struct to the first
    /// `CapDescriptor` entry.
    ///
    /// The descriptor array describes each initial capability so the
    /// process can identify what each slot represents without probing.
    pub cap_descriptors_offset: u32,

    // ── Startup message ─────────────────────────────────────────────

    /// Byte offset from the start of this struct to the startup message.
    /// Zero if no startup message is present.
    ///
    /// The startup message is an opaque byte sequence provided by the
    /// creating service. Typical contents: service name, configuration
    /// parameters, or a serialised argument structure.
    pub startup_message_offset: u32,

    /// Length of the startup message in bytes. Zero if absent.
    pub startup_message_len: u32,

    /// Padding to maintain 8-byte alignment for the trailing
    /// `CapDescriptor` array.
    pub _pad: u32,
}
```

The `CapDescriptor` type is shared with `abi/init-protocol` (or defined
identically) so that both init and normal processes use the same descriptor
format for identifying capabilities by type and metadata.

### Fixed CSpace slot conventions

Slots 0 through `initial_caps_base - 1` have fixed assignments:

| Slot | Content |
|---|---|
| 0 | Null (permanently invalid, per capability model) |
| `self_thread_cap` | Thread capability (Control) |
| `self_aspace_cap` | AddressSpace capability |
| `self_cspace_cap` | CSpace capability |
| `creator_endpoint_cap` | Endpoint to creating service (if nonzero) |

Slots from `initial_caps_base` onward are service-specific and described by the
`CapDescriptor` array.

---

## StartupInfo

The struct passed to `main()`. This is a Rust-native type (NOT `#[repr(C)]`)
providing ergonomic access to the handover data. It is constructed by `_start()`
from either `ProcessInfo` (normal processes) or `InitInfo` (init/ktest).

```rust
pub struct StartupInfo<'a> {
    /// Capability descriptors for initial capabilities.
    pub initial_caps: &'a [CapDescriptor],

    /// Virtual address of the IPC buffer page.
    pub ipc_buffer: *mut u8,

    /// CSpace slot of the parent endpoint. Zero if none.
    pub creator_endpoint: u32,

    /// Startup message bytes. Empty slice if none.
    pub startup_message: &'a [u8],

    /// CSpace slot of own Thread capability.
    pub self_thread: u32,

    /// CSpace slot of own AddressSpace capability.
    pub self_aspace: u32,

    /// CSpace slot of own CSpace capability.
    pub self_cspace: u32,
}
```

`StartupInfo` borrows from the handover page — the page remains mapped
(read-only) for the lifetime of the process, so the borrow is valid.

---

## main() Signature

All Seraph userspace binaries MUST define `main` with the following signature:

```rust
fn main(startup: &StartupInfo) -> !
```

`main()` receives a reference to the `StartupInfo` constructed by `_start()`.
It MUST NOT return — processes terminate by calling `sys_thread_exit`. If
`main()` could return, the `_start()` stub calls `sys_thread_exit(0)` as a
safety net.

A runtime crate (anticipated in `shared/`) will provide the `_start()`
implementation that:

1. Reads the handover struct from the well-known virtual address
2. Validates the protocol version
3. Constructs `StartupInfo` from the handover fields
4. Calls `main()`
5. Calls `sys_thread_exit` if `main()` returns (defensive; should not happen)

Two `_start()` variants will exist:
- **Normal process `_start()`** — reads `ProcessInfo` from `PROCESS_INFO_VADDR`
- **Init/ktest `_start()`** — reads `InitInfo` from `INIT_INFO_VADDR`
  (defined in `abi/init-protocol`)

Both produce the same `StartupInfo` and call the same `main()`.

---

## Relationship to init-protocol

| | init-protocol | process-abi |
|---|---|---|
| Producer | Kernel (Phase 9) | procmgr |
| Consumer | init, ktest | All other processes |
| Handover struct | `InitInfo` | `ProcessInfo` |
| Placed at | `INIT_INFO_VADDR` | `PROCESS_INFO_VADDR` |
| Contains platform-global state | Yes (all memory frames, all HW caps, firmware tables) | No |
| Contains parent endpoint | No (init has no parent) | Yes |
| `main()` signature | Same (`&StartupInfo`) | Same (`&StartupInfo`) |

Init-protocol is a kernel-internal concern — it carries the full initial CSpace
layout including platform resources that only init needs. Process-abi carries
only what a single service or application requires.

The `CapDescriptor` type SHOULD be shared between the two crates (via a common
dependency or identical definition) to avoid divergence.

---

## Versioning

`PROCESS_ABI_VERSION` MUST be incremented on any breaking change to the
`ProcessInfo` layout or field semantics. This mirrors the versioning discipline
in `abi/init-protocol`.

`StartupInfo` and the `main()` signature are source-level conventions, not
binary ABI. Changes to them require recompilation but not a version bump —
they are always compiled together with the consuming binary.

---

## Relevant Design Documents

| Document | Content |
|---|---|
| [docs/capability-model.md](../../docs/capability-model.md) | Capability types, CSpace, derivation, rights |
| [docs/ipc-design.md](../../docs/ipc-design.md) | IPC buffer, message format, endpoints |
| [docs/architecture.md](../../docs/architecture.md) | Bootstrap sequence, procmgr role |
| [abi/init-protocol](../init-protocol/README.md) | Kernel-to-init handover contract |

---

## Summarized By

None
