# Capability Model

## Overview

Capabilities are the sole access control mechanism in Seraph.

- Every kernel-managed resource is represented by a capability.
- A process must not operate on a resource without a valid capability authorising
  that operation.
- The kernel must enforce capability checks on every resource operation.
- The system must not provide ambient authority or identity-based privilege.
- A resource must not be accessible by naming or guessing an identifier without
  holding the corresponding capability.

---

## Capability Spaces

Each process has a **capability space** (CSpace): a collection of capability slots.
Slots are referenced by integer index — a capability descriptor. A slot is either
empty (the null capability) or holds one capability referencing a kernel object and
its associated rights.

The CSpace has the following properties:
- **Grows on demand** — starts small and expands as slots are needed, without
  requiring the process to predict its capability requirements upfront
- **Stable indices** — a capability descriptor remains valid for the lifetime of
  the capability; the kernel never moves or renumbers existing slots
- **O(1) lookup** — descriptor-to-capability resolution must be fast, as it occurs
  on every IPC call and resource operation
- **Per-process ceiling** — each process has a maximum CSpace size enforced by the
  kernel, preventing any process from exhausting kernel memory by accumulating slots

Slot 0 is permanently null and cannot be written — using index 0 always means "no
capability".

---

## Capability Types

Each capability type represents a distinct kind of kernel object. The rights attached
to a capability are type-specific.

### Memory Frame

A capability to one or more contiguous physical frames. Rights:
- **Map** — may map these frames into an address space
- **Write** — mapped region is writable
- **Execute** — mapped region is executable

W^X is enforced: Write and Execute rights may not both be present on the same
capability. The kernel rejects any attempt to create such a capability.

### Address Space

A capability to a process's virtual address space. Rights:
- **Map** — may install and remove mappings in this address space
- **Read** — may inspect current mappings

The kernel holds implicit authority over all address spaces; this capability is
what allows userspace memory managers to manage mappings on behalf of a process.

### IPC Endpoint

A capability to an IPC endpoint. Rights:
- **Send** — may call this endpoint (synchronous IPC, caller blocks for reply)
- **Receive** — may accept calls on this endpoint (held only by the server)
- **Grant** — may include capabilities in the message's capability slots

A send capability without grant right cannot pass capabilities to the server.
A server that should not receive unexpected resources from clients holds a receive
capability without grant on its own endpoint.

### Signal

A capability to a signal object (bitmask-based async notification). Rights:
- **Signal** — may OR bits into the signal word (deliver notifications)
- **Wait** — may wait on this signal object and read the bitmask

### Event Queue

A capability to an event queue (ordered ring buffer). Rights:
- **Post** — may append an entry to the queue
- **Recv** — may wait on and read entries from the queue

### Interrupt

A capability granting the right to handle a specific hardware interrupt line.
The holder registers an endpoint to receive interrupt notifications on that line.
Interrupt capabilities are created by the kernel at boot and initially granted to
init, which delegates them to appropriate drivers.

### MMIO Region

A capability to a specific physical address range used for memory-mapped I/O.
Holding this capability allows mapping the region into an address space (with Map
right). Without this capability a process cannot map physical addresses — it cannot
name hardware it has not been granted access to.

### Thread

A capability to a thread. Rights:
- **Control** — may start, stop, and configure the thread
- **Observe** — may read the thread's register state (for debugging)

### Process

A capability to a process. Rights:
- **Control** — may terminate the process and manage its CSpace
- **Supervise** — may receive lifecycle events from the process

### Wait Set

A capability to a wait set (see IPC design). Rights:
- **Modify** — may add or remove members
- **Wait** — may block on the wait set

### IoPortRange (x86-64 only)

A capability to a contiguous range of x86 I/O port numbers. Rights:
- **Use** — may bind this port range to a thread, allowing that thread to execute
  `in`/`out` instructions for those ports without a syscall

IoPortRange capabilities are created at boot from `IoPortRange` entries in the
boot-provided `platform_resources`. They are not creatable at runtime. A driver
that needs port I/O access receives a derived IoPortRange capability from init
(via devmgr), covering only its assigned port range.

Revoking an IoPortRange capability removes port access from all threads it has
been bound to. The kernel tracks bindings and updates each affected thread's IOPB
in the TSS on revocation.

### SchedControl

A capability granting authority to assign elevated scheduling priorities. Rights:
- **Elevate** — may set thread priorities in the elevated range

There is one SchedControl capability, created at boot. Init holds it and delegates
derived copies to services that need real-time-ish scheduling (e.g. audio servers,
device managers). Without a SchedControl capability, a process can only set thread
priorities in the normal range. For priority levels, ranges, and constants, see
[kernel/docs/scheduler.md § Priority Levels](../kernel/docs/scheduler.md#priority-levels).

---

## Rights and Attenuation

Rights are a bitmask attached to each capability slot. When deriving a capability,
the derived copy may have equal or fewer rights than the source — rights can only
be removed, never added. This is called **attenuation**.

A process cannot grant another process more authority than it holds itself. If a
process holds a send-only endpoint capability, it can derive another send-only
capability (or a weaker one with no grant right), but it cannot produce a receive
capability it does not hold.

The kernel enforces this at derivation time. Any attempt to derive a capability
with rights not present in the source is rejected.

---

## Derivation and the Derivation Tree

Capabilities may be derived: a new capability slot is created referencing the same
underlying object, with equal or fewer rights. The original is retained. Both slots
now reference the object independently.

The kernel maintains a **derivation tree** tracking the parent/child relationships
between capability slots across all processes. This tree is what makes revocation
work correctly.

Derivation is the mechanism by which authority is delegated. Init holds broad
capabilities at boot and derives narrower ones to pass to services. A service derives
narrower ones still to pass to its clients. Each derivation is a deliberate,
attenuated grant of access.

---

## Transfer

A capability may be transferred via IPC (see [ipc-design.md](ipc-design.md)). Transfer
moves the capability from the sender's CSpace to the receiver's CSpace — the sender's
slot becomes null. This is not derivation; no new entry appears in the derivation tree.
The receiver inherits the sender's position in the existing tree.

Transfer is how resources change hands without duplication. A server that grants a
file handle to a client has genuinely given it up.

---

## Revocation

Any process may revoke a capability it has derived. Revocation:

1. Invalidates the target capability slot
2. Recursively invalidates all capabilities derived from it, in all processes

After revocation, any process that held a derived capability can no longer use it.
The underlying kernel object is not destroyed — only the authority to access it is
withdrawn. If the revoker still holds the parent capability, it retains access.

Revocation is the mechanism by which a resource can be safely reclaimed or
reassigned. A server that lends a shared memory region to a client can revoke
the client's capability when the session ends, without the client's cooperation.

The kernel must be able to find and invalidate all derived capabilities efficiently.
This is the primary reason the derivation tree is maintained.

---

## Object Creation

New kernel objects are created via typed syscalls. Each object type has a
corresponding creation call:

```
create_endpoint()   → endpoint_cap (Send + Receive + Grant)
create_signal()     → signal_cap   (Signal + Wait)
create_event_queue(capacity) → queue_cap (Post + Recv)
create_thread(...)  → thread_cap   (Control)
create_address_space() → aspace_cap (Map)
create_wait_set()   → wait_set_cap (Modify + Wait)
```

The returned capability is placed in a free slot in the caller's CSpace. The caller
holds all rights on a freshly created object. It may then derive and delegate narrower
capabilities to other processes as appropriate.

The kernel does not track ownership beyond the derivation tree. If a process destroys
all capabilities in the derivation tree for an object — including its own — the kernel
frees the object. Objects do not outlive all references to them.

---

## Initial Capability Distribution

At boot, the kernel creates the init process and populates its CSpace with an initial
set of capabilities covering all available resources:

- Frame capabilities for all usable physical memory
- MMIO region capabilities for all boot-provided platform resource regions
  (MmioRange, PciEcam, IommuUnit entries from `BootInfo.platform_resources`)
- Interrupt capabilities for all boot-provided interrupt lines
- Read-only Frame capabilities for firmware table regions (PlatformTable entries),
  allowing userspace to parse ACPI or Device Tree data
- IoPortRange capabilities for all boot-provided I/O port ranges (x86-64 only)
- One SchedControl capability (Elevate rights)
- Thread and process capabilities for init itself

Init is responsible for delegating appropriate subsets of this authority to each service it starts,
following the principle of least privilege. See [device-management.md](device-management.md#what-devmgr-receives-from-init)
for devmgr's specific initial capability set.

This is the only point at which capabilities are created from nothing. All subsequent
authority in the system is derived from this initial grant.

---

## What the Kernel Does Not Do

The kernel does not provide:
- **Ambient authority** — there is no "root" or "superuser" at the kernel level.
  Init holds broad authority by virtue of its initial capabilities, not by identity.
- **Capability lookup by name** — there is no global namespace of capabilities.
  A process receives capabilities from its parent or via IPC; it cannot search for them.
- **Policy** — the kernel enforces that operations are authorised by capability.
  What the capabilities represent and how they should be distributed is entirely
  a userspace concern, managed by init and the services it supervises.
