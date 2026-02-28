# IPC Design

## Overview

IPC is the backbone of Seraph. All communication between processes — service requests,
event delivery, resource passing — goes through the kernel's IPC mechanism. There are
no side channels; two processes that do not share an IPC capability cannot communicate.

Seraph uses a hybrid model: **synchronous calls** for structured request/reply between
services, and two **asynchronous primitives** for event delivery — signals and event
queues. Each primitive is simple and honest about its semantics.

---

## Synchronous IPC

### Endpoints

An endpoint is a kernel object through which synchronous IPC occurs. It is created by
a server and referenced by capability. Holding a send capability to an endpoint allows
a process to call the server; only the process holding the receive capability can accept
calls on it.

Endpoints themselves carry no state between calls. They are rendezvous points — a call
blocks until the server is ready to receive, and a receive blocks until a caller arrives.

### The Call/Reply Model

Synchronous IPC follows a strict call/reply pattern:

1. **Caller** invokes `call(endpoint_cap, message)` and blocks.
2. **Server** invokes `recv(endpoint_cap)`, which returns the message and a
   single-use **reply capability**.
3. **Server** processes the request and invokes `reply(reply_cap, message)`.
4. **Caller** is unblocked and receives the reply.

The reply capability is granted by the kernel at receive time and is valid for exactly
one use. It cannot be stored, delegated, or reused. This enforces the invariant that
every call receives exactly one reply and prevents servers from replying to stale or
incorrect callers.

A server that needs to delegate work before replying — passing a request on to another
service — may save its own reply capability and reply only after receiving the
downstream result. This composes correctly without any special kernel support.

### Message Format

A message consists of:

- **Label** — one word. Interpreted by the receiver as a message type or opcode.
  The kernel does not inspect or validate the label.
- **Data words** — up to `MSG_DATA_WORDS_MAX` words carrying the message payload.
- **Capability slots** — up to `MSG_CAP_SLOTS_MAX` capability references.
  Capabilities in these slots are transferred from sender to receiver atomically
  with the message. The sender loses access to transferred capabilities.

**Small messages (fast path):** When the data word count fits within the register
budget (`MSG_REGS_DATA_MAX` words), the entire message — label, data, and capability
slots — passes through kernel-mediated register state. No memory access occurs after
argument validation. No dynamic allocation occurs. "No dynamic allocation in the IPC
path" holds for this common case.

**Extended payloads:** When a message exceeds the register budget, the additional data
words spill to a per-thread **IPC buffer page**. Each thread registers its IPC buffer
page once via `SYS_IPC_BUFFER_SET`. The kernel reads from the sender's page and writes
to the receiver's page directly — no arbitrary user pointer dereference, no heap
allocation. If the IPC buffer page is not registered or is unmapped at the time of an
extended IPC, the syscall fails with `InvalidArgument`. Capability slots always travel
in registers regardless of payload size.

Extended payloads are intended for cases where small messages are insufficient but
shared memory (see Large Data Transfers below) would be premature. For bulk data,
shared memory remains the correct approach.

### Large Data Transfers

Fixed-size messages are intentionally small. For large payloads — bulk data, file
contents, frame buffers — the correct approach is to pass a shared memory capability
rather than embedding data in the message.

The sender maps a memory region, writes data into it, and passes a capability to that
region in the message's capability slots. The receiver maps the region into its own
address space. No kernel copy occurs. The capability controls which process can access
the region and with what rights (read-only, read-write).

This is not a workaround — it is the intended design. Large data transfer via shared
memory is faster than any copy-based scheme and composes naturally with the capability
model.

---

## Asynchronous Primitives

Asynchronous notification is needed wherever the sender must not block — hardware
interrupt delivery, timers, completion signals. Two distinct primitives cover the
two meaningfully different cases.

### Signals

A signal is a kernel object containing a single machine word used as a bitmask. Each
bit represents a distinct event type, defined by the service using the signal.

**Delivery:** The sender ORs one or more bits into the signal word. This is O(1) and
never blocks. If the receiver is already waiting, it is woken immediately. If not,
the bits accumulate until the receiver next waits.

**Coalescing:** Setting an already-set bit is idempotent. If an interrupt fires three
times before the driver wakes, the driver sees the bit set once and handles the
hardware state — which is the correct behaviour, since it reads hardware registers
to determine what needs handling regardless.

**Receipt:** The receiver waits on the signal object and receives the full bitmask,
which is atomically cleared on read. The receiver then inspects each set bit and
acts accordingly.

Signals are appropriate for: hardware interrupt delivery, timer expiry, IPC endpoint
readiness, DMA completion, and any event where what matters is that something happened,
not how many times or in what order.

### Event Queues

An event queue is a fixed-capacity ring buffer. Each entry carries a word-sized payload.
The capacity is chosen at creation time and does not change.

**Delivery:** The sender appends an entry to the ring. If the ring is full, the send
returns an error — it is the sender's responsibility to handle backpressure. Delivery
is otherwise O(1) and non-blocking.

**Ordering:** Entries are delivered to the receiver in the order they were posted.
Events are not coalesced. "Process A exited, then process B exited" is preserved
as two distinct entries in order.

**Receipt:** The receiver waits on the queue and receives the next available entry.
If multiple entries are available, subsequent receives return them in order without
blocking.

Event queues are appropriate for: process lifecycle events (exit, signal delivery),
anything where ordering or count of events matters, and cases where coalescing would
cause correctness problems.

### Why Two Primitives

A single notification primitive either coalesces (losing ordering, wrong for process
events) or does not coalesce (adding ring buffer overhead to the common interrupt case).
Signals and event queues each have honest, well-defined semantics. The choice between
them at a given site is a deliberate statement about whether ordering matters.

---

## Waiting on Multiple Sources

A process often needs to wait for input from several sources simultaneously — a service
handling multiple clients, a driver waiting for either an interrupt or a timeout, a
shell waiting for input or a child process to exit.

Seraph provides a **wait set**: a kernel object that aggregates any combination of
endpoints, signals, and event queues. A process waits on the wait set and is woken
when any member becomes ready. The wait returns an identifier indicating which source
triggered the wake, after which the process reads from that source normally.

This covers multiplexed I/O and event-driven patterns without requiring per-source
polling or separate threads.

---

## Capability Semantics in IPC

IPC capabilities carry three rights — Send (call the endpoint), Receive (accept calls), and Grant
(pass capabilities in messages) — with scoping rules defined in [capability-model.md#ipc-endpoint](capability-model.md#ipc-endpoint).
Capabilities passed in IPC messages are moved, not copied; see [capability-model.md#transfer](capability-model.md#transfer).

---

## Kernel Role

The kernel delivers messages, manages endpoint queuing, and transfers capability
references atomically with messages. It has no opinion on message content, service
protocols, or what capabilities mean to the receiving process.

The kernel does not provide:
- Service discovery — processes must be given endpoint capabilities by their parent
  or by a trusted intermediary (init)
- Protocol versioning or negotiation — these are userspace concerns
- Broadcast or multicast — one sender, one receiver per endpoint call

These are deliberate omissions. Each belongs in userspace where it can be reasoned
about, tested, and changed without touching the trusted computing base.
