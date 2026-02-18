# Syscall Interface Specification

## Overview

This document defines the complete syscall ABI for Seraph: calling convention,
entry/exit mechanism, the full syscall table, per-call argument and return specifications,
error codes, and atomicity guarantees.

The syscall interface is the kernel's only public API. Every operation a userspace
program can perform that touches a kernel-managed resource goes through this table.

---

## Calling Convention

### x86-64

Seraph uses the `SYSCALL`/`SYSRET` instructions on x86-64.

| Register | Role |
|---|---|
| `rax` | Syscall number (in); return value (out) |
| `rdi` | Argument 0 |
| `rsi` | Argument 1 |
| `rdx` | Argument 2 |
| `r10` | Argument 3 (not `rcx` — `SYSCALL` clobbers `rcx` with the return address) |
| `r8` | Argument 4 |
| `r9` | Argument 5 |
| `rcx` | Clobbered by `SYSCALL` (holds return address); not an argument register |
| `r11` | Clobbered by `SYSCALL` (holds saved rflags); not an argument register |

All other registers are preserved across a syscall. The callee-saved set matches the
System V AMD64 ABI (`rbx`, `rbp`, `r12`–`r15`).

**Return values:**

- `rax`: primary return value. On error, `rax` holds a negative `SyscallError` code.
  On success, `rax` holds the non-negative result (or zero if the call has no result).
- `rdx`: secondary return value, used by calls that return two values (e.g. `ipc_recv`
  returns both a label and a word count). Zero if unused.

**Errno convention:** The kernel returns the error code directly in `rax` as a
negative `i64`. There is no `errno` global — callers check the sign of `rax`.

### RISC-V

Seraph uses the `ECALL` instruction on RISC-V. The trap handler in `stvec` dispatches
`ecall` from U-mode to the syscall path.

| Register | Role |
|---|---|
| `a7` | Syscall number (in) |
| `a0` | Argument 0; primary return value (out) |
| `a1` | Argument 1; secondary return value (out) |
| `a2` | Argument 2 |
| `a3` | Argument 3 |
| `a4` | Argument 4 |
| `a5` | Argument 5 |

All other registers are preserved. Callee-saved registers are `s0`–`s11`, `sp`, `ra`
(matching the RISC-V calling convention).

**Return values:** `a0` is the primary return value (negative on error). `a1` is the
secondary return value where applicable.

---

## Syscall Entry and Exit

### x86-64

On `SYSCALL`:
1. `rcx` ← `rip` (return address); `r11` ← `rflags`
2. Transition to CPL 0 with kernel code segment
3. `rsp` ← kernel stack pointer from `RSP0` in the TSS (per-CPU)
4. Kernel saves the user register file (including `rcx` and `r11`) onto the kernel stack
5. Kernel calls `syscall::dispatch(nr=rax, args=[rdi, rsi, rdx, r10, r8, r9])`
6. Kernel writes return values into the saved register frame
7. Kernel restores the user register file
8. `SYSRET` restores `rip` from `rcx`, `rflags` from `r11`, transitions to CPL 3

Interrupts are disabled by `SFMASK` on `SYSCALL` entry (the `IF` bit is cleared).
The kernel re-enables them after saving state and switching to the kernel stack.

### RISC-V

On `ECALL` from U-mode:
1. `sepc` ← `pc` + 4 (return address past the ecall instruction)
2. `sstatus.SPP` ← 0 (was U-mode); `sstatus.SPIE` ← `sstatus.SIE`; `sstatus.SIE` ← 0
3. Execution jumps to `stvec` (the kernel trap handler)
4. Trap handler saves the full user register file to the per-thread trap frame
5. Trap handler checks `scause` — if it is an ecall from U-mode, routes to syscall path
6. Kernel calls `syscall::dispatch(nr=a7, args=[a0..a5])`
7. Kernel writes return values into the saved register frame (`a0`, `a1`)
8. Kernel restores the user register file
9. `SRET` restores `pc` from `sepc`, restores `sstatus.SIE` from `sstatus.SPIE`,
   returns to U-mode (`sstatus.SPP` = 0)

---

## Syscall Numbers

Syscall numbers are stable ABI. New syscalls are added at the end; existing numbers
are never reassigned or reused.

```
0   SYS_IPC_CALL
1   SYS_IPC_REPLY
2   SYS_IPC_RECV
3   SYS_SIGNAL_SEND
4   SYS_SIGNAL_WAIT
5   SYS_EVENT_POST
6   SYS_EVENT_RECV
7   SYS_CAP_CREATE_ENDPOINT
8   SYS_CAP_CREATE_SIGNAL
9   SYS_CAP_CREATE_EVENT_QUEUE
10  SYS_CAP_CREATE_THREAD
11  SYS_CAP_CREATE_ADDRESS_SPACE
12  SYS_CAP_CREATE_WAIT_SET
13  SYS_CAP_DERIVE
14  SYS_CAP_REVOKE
15  SYS_CAP_DELETE
16  SYS_MEM_MAP
17  SYS_MEM_UNMAP
18  SYS_MEM_PROTECT
19  SYS_THREAD_START
20  SYS_THREAD_STOP
21  SYS_THREAD_YIELD
22  SYS_PROCESS_EXIT
23  SYS_WAIT_SET_ADD
24  SYS_WAIT_SET_REMOVE
25  SYS_WAIT_SET_WAIT
26  SYS_IRQ_ACK
27  SYS_CAP_CREATE_PROCESS
28  SYS_CAP_INSERT
29  SYS_IRQ_REGISTER
30  SYS_FRAME_SPLIT
31  SYS_PROCESS_KILL
32  SYS_PROCESS_SUPERVISE
33  SYS_MMIO_MAP
34  SYS_IOPB_GRANT
35  SYS_DMA_GRANT
36  SYS_THREAD_SET_PRIORITY
37  SYS_THREAD_SET_AFFINITY
38  SYS_THREAD_READ_REGS
39  SYS_THREAD_WRITE_REGS
40  SYS_ASPACE_QUERY
41  SYS_IPC_BUFFER_SET
```

---

## Error Codes

All syscalls return one of these error codes on failure. The value is negative in
`rax`/`a0`. Zero and positive values are success.

```rust
#[repr(i64)]
pub enum SyscallError
{
    /// Capability descriptor does not refer to a valid capability.
    InvalidCapability  = -1,
    /// The capability does not have the required rights for this operation.
    AccessDenied       = -2,
    /// An argument value is out of range or otherwise invalid.
    InvalidArgument    = -3,
    /// A required memory allocation failed.
    OutOfMemory        = -4,
    /// The target endpoint has no receiver waiting (non-blocking variant only).
    WouldBlock         = -5,
    /// The event queue is full; the post was rejected.
    QueueFull          = -6,
    /// The referenced object is in a state that does not permit this operation.
    InvalidState       = -7,
    /// The syscall number is not recognised.
    UnknownSyscall     = -8,
    /// The operation was interrupted (e.g. thread stopped while blocked).
    Interrupted        = -9,
    /// A physical address or virtual address argument is not aligned or canonical.
    AlignmentError     = -10,
    /// The requested mapping would exceed the address space's limit.
    AddressSpaceFull   = -11,
}
```

---

## IPC Syscalls

### `SYS_IPC_CALL` (0)

Send a message to an endpoint and block until a reply is received.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `endpoint_cap` | Send capability to an IPC endpoint |
| 1 | `label` | Message label (opaque word; passed to server as-is) |
| 2 | `data_count` | Number of data words (0–MSG_DATA_WORDS_MAX) |
| 3 | `cap_slots` | Packed descriptor: up to MSG_CAP_SLOTS_MAX caps to transfer |
| 4 | `flags` | Bit 0: extended payload in IPC buffer page (see below) |

`cap_slots` encodes up to `MSG_CAP_SLOTS_MAX` capability descriptors packed into one
word (implementation constant; expected value 4, requiring 16 bits each in a 64-bit
word for up to 4 caps).

**Small messages (fast path):** When `data_count` ≤ `MSG_REGS_DATA_MAX` and
`flags` bit 0 is clear, all data words pass in registers. No memory access occurs
after argument validation.

**Extended payload:** When `flags` bit 0 is set, data words beyond the register
capacity are read from the caller's IPC buffer page (registered via
`SYS_IPC_BUFFER_SET`). The kernel reads directly from that page; no arbitrary pointer
dereference occurs. Reply data beyond register capacity is written to the caller's
IPC buffer page after the server replies.

**Return:**

- `rax`/`a0`: 0 on success; `SyscallError` on failure
- `rdx`/`a1`: reply label (valid on success)

**Capability requirement:** `endpoint_cap` must have Send rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (bad count,
or extended payload requested but IPC buffer page not registered or unmapped),
`Interrupted`.

---

### `SYS_IPC_REPLY` (1)

Send a reply to the caller that issued the most recent `SYS_IPC_RECV` on this thread.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `label` | Reply label |
| 1 | `data_count` | Number of data words (0–MSG_DATA_WORDS_MAX) |
| 2 | `cap_slots` | Capabilities to transfer in the reply (packed descriptors) |
| 3 | `flags` | Bit 0: extended payload in IPC buffer page |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The reply capability is implicit — it is retrieved from the calling thread's
`reply_cap_slot` (a per-thread field outside the CSpace, set at `SYS_IPC_RECV`
time). It is consumed by this syscall whether it succeeds or fails. If no reply
capability is present (i.e. this thread did not receive a call), the syscall
returns `InvalidCapability`.

Extended payload follows the same rules as `SYS_IPC_CALL`: when `flags` bit 0 is
set, data beyond register capacity is read from this thread's IPC buffer page and
written to the original caller's IPC buffer page.

**Capability requirement:** Implicit reply capability from `current_tcb.reply_cap_slot`.

**Errors:** `InvalidCapability` (no pending reply), `InvalidArgument`, `Interrupted`.

---

### `SYS_IPC_RECV` (2)

Wait for a call on an endpoint. Blocks until a caller arrives.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `endpoint_cap` | Receive capability to an IPC endpoint |

**Return:**

- `rax`/`a0`: 0 on success; `SyscallError` on failure
- `rdx`/`a1`: label from the incoming message

Data words up to `MSG_REGS_DATA_MAX` are returned in registers. Extended payload
(when the sender set `flags` bit 0) is written to the receiver's IPC buffer page.
The kernel places a reply capability into a per-thread slot (`reply_cap_slot`);
this capability is retrieved implicitly by `SYS_IPC_REPLY`.

**Capability requirement:** `endpoint_cap` must have Receive rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `Interrupted`.

---

### `SYS_SIGNAL_SEND` (3)

OR bits into a signal object. Non-blocking; wakes the waiter if one is present.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `signal_cap` | Signal capability with Signal rights |
| 1 | `bits` | Bitmask to OR into the signal word |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

**Capability requirement:** `signal_cap` must have Signal rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (bits == 0).

---

### `SYS_SIGNAL_WAIT` (4)

Block until at least one bit is set in the signal object. Returns and atomically
clears the entire bitmask.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `signal_cap` | Signal capability with Wait rights |

**Return:**

- `rax`/`a0`: the bitmask that was set (positive, non-zero) on success;
  `SyscallError` on failure

**Capability requirement:** `signal_cap` must have Wait rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `Interrupted`.

---

### `SYS_EVENT_POST` (5)

Append one entry to an event queue. Non-blocking; returns `QueueFull` if at capacity.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `queue_cap` | Event queue capability with Post rights |
| 1 | `payload` | Word-sized payload to append |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

**Capability requirement:** `queue_cap` must have Post rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `QueueFull`.

---

### `SYS_EVENT_RECV` (6)

Wait for and dequeue the next entry from an event queue.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `queue_cap` | Event queue capability with Recv rights |

**Return:**

- `rax`/`a0`: 0 on success; `SyscallError` on failure
- `rdx`/`a1`: dequeued payload word (valid on success)

**Capability requirement:** `queue_cap` must have Recv rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `Interrupted`.

---

## Capability Syscalls

### `SYS_CAP_CREATE_ENDPOINT` (7)

Create a new IPC endpoint. Returns a capability with Send + Receive + Grant rights.

**Arguments:** None (no arguments required).

**Return:**

- `rax`/`a0`: new capability descriptor on success (positive); `SyscallError` on failure

**Errors:** `OutOfMemory` (cannot allocate endpoint object or CSpace slot).

---

### `SYS_CAP_CREATE_SIGNAL` (8)

Create a new signal object. Returns a capability with Signal + Wait rights.

**Arguments:** None.

**Return:** `rax`/`a0`: new capability descriptor on success; `SyscallError` on failure.

**Errors:** `OutOfMemory`.

---

### `SYS_CAP_CREATE_EVENT_QUEUE` (9)

Create a new event queue with a fixed capacity.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `capacity` | Ring buffer capacity in entries (1–EVENT_QUEUE_MAX_CAPACITY) |

**Return:** `rax`/`a0`: new capability descriptor (Post + Recv rights) on success;
`SyscallError` on failure.

**Errors:** `OutOfMemory`, `InvalidArgument` (capacity 0 or exceeds maximum).

---

### `SYS_CAP_CREATE_THREAD` (10)

Create a new thread in an existing address space.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Map rights) for the new thread |
| 1 | `entry` | Virtual address of the thread entry point |
| 2 | `stack_top` | Initial stack pointer |
| 3 | `arg` | Value passed in first argument register |
| 4 | `priority` | Scheduling priority (0 = lowest; PRIORITY_MAX = highest user level) |

**Return:** `rax`/`a0`: new thread capability (Control rights) on success;
`SyscallError` on failure.

The thread is created in the `Created` state; it does not begin execution until
`SYS_THREAD_START` is called.

**Capability requirement:** `aspace_cap` must have Map rights. Map is intentionally
reused here: a process that can modify an address space's mappings is inherently
trusted to create threads that execute within it.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (bad entry, stack,
or priority), `OutOfMemory`.

---

### `SYS_CAP_CREATE_ADDRESS_SPACE` (11)

Create a new, empty address space. The kernel's higher-half mapping is shared into
the new address space automatically.

**Arguments:** None.

**Return:** `rax`/`a0`: new address space capability (Map + Read rights) on success;
`SyscallError` on failure.

**Errors:** `OutOfMemory`.

---

### `SYS_CAP_CREATE_WAIT_SET` (12)

Create a new wait set.

**Arguments:** None.

**Return:** `rax`/`a0`: new wait set capability (Modify + Wait rights) on success;
`SyscallError` on failure.

**Errors:** `OutOfMemory`.

---

### `SYS_CAP_DERIVE` (13)

Derive a new capability from an existing one, with equal or fewer rights.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `source_cap` | Source capability descriptor |
| 1 | `rights_mask` | Rights bitmask for the derived capability (subset of source) |

**Return:** `rax`/`a0`: new capability descriptor on success; `SyscallError` on failure.

The derived capability references the same kernel object. The derivation is recorded
in the global derivation tree for revocation tracking.

**Errors:** `InvalidCapability` (source invalid or null), `AccessDenied` (requested
rights exceed those held in source), `OutOfMemory` (no free CSpace slot).

---

### `SYS_CAP_REVOKE` (14)

Revoke a capability and all capabilities derived from it, across all processes.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `cap` | Capability to revoke |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The capability itself is invalidated, as are all descendants in the derivation tree.
The underlying kernel object is not freed unless this was the last reference to it.

**Errors:** `InvalidCapability`.

---

### `SYS_CAP_DELETE` (15)

Delete a single capability from the caller's CSpace. Does not affect derived capabilities.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `cap` | Capability descriptor to delete |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

If this is the last reference to the underlying object, the object is freed.

**Errors:** `InvalidCapability`.

---

## Memory Syscalls

### `SYS_MEM_MAP` (16)

Map a physical frame capability into an address space.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Map rights) |
| 1 | `frame_cap` | Frame capability (Map rights) to map |
| 2 | `virt` | Virtual address to map at (page-aligned) |
| 3 | `flags` | Mapping flags: readable, writable, executable, user-accessible |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

W^X is enforced: `flags` may not specify both writable and executable. The frame's
rights are also checked — if `frame_cap` does not have Write rights, the mapping
cannot be writable.

**Capability requirements:** `aspace_cap` (Map), `frame_cap` (Map).

**Errors:** `InvalidCapability`, `AccessDenied` (rights mismatch or W^X violation),
`InvalidArgument` (unaligned `virt` or non-canonical address), `AlignmentError`,
`AddressSpaceFull`.

---

### `SYS_MEM_UNMAP` (17)

Remove a mapping from an address space.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Map rights) |
| 1 | `virt` | Virtual address to unmap (page-aligned) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The physical frame is not freed — only the virtual mapping is removed. The frame
capability continues to exist. TLB shootdowns are performed on all CPUs running
threads in `aspace_cap`.

**Capability requirement:** `aspace_cap` must have Map rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (address not
mapped or unaligned).

---

### `SYS_MEM_PROTECT` (18)

Change the permission flags on an existing mapping without altering the physical address.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Map rights) |
| 1 | `virt` | Virtual address of the mapping (page-aligned) |
| 2 | `flags` | New permission flags |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

W^X is enforced on the new flags. The caller cannot grant rights beyond what the
frame capability allows (but the frame capability is not re-checked here — the kernel
records the maximum rights at map time).

**Capability requirement:** `aspace_cap` must have Map rights.

**Errors:** `InvalidCapability`, `AccessDenied` (W^X violation or rights exceed
initial mapping rights), `InvalidArgument` (address not mapped).

---

## Thread and Process Syscalls

### `SYS_THREAD_START` (19)

Transition a thread from `Created` state to `Ready` and enqueue it for scheduling.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Control rights) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

**Capability requirement:** `thread_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidState` (thread not in
Created state).

---

### `SYS_THREAD_STOP` (20)

Stop a running or runnable thread. The thread transitions to `Stopped` state.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Control rights) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

If the thread is blocked on IPC, the block is cancelled (the blocked syscall on the
target thread returns `Interrupted`). If the thread is running on another CPU, an
inter-processor interrupt is sent to force it out of userspace.

**Capability requirement:** `thread_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidState` (thread already stopped
or exited).

---

### `SYS_THREAD_YIELD` (21)

Voluntarily yield the remainder of the current thread's time slice.

**Arguments:** None.

**Return:** `rax`/`a0`: always 0.

The calling thread remains `Ready` and is re-enqueued at its current priority. No
capability is required — this syscall acts on the calling thread implicitly.

---

### `SYS_PROCESS_EXIT` (22)

Exit the calling process. All threads are stopped, all capabilities are deleted,
and all resources are freed.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `exit_code` | Exit code delivered to the process supervisor (if any) |

**Return:** Does not return.

This syscall never returns to the caller. The kernel terminates the process and
schedules another thread. If the process has a supervisor holding a Supervise
capability, a process-exit event is posted to its event queue.

**Errors:** None (this syscall cannot fail).

---

## Wait Set Syscalls

### `SYS_WAIT_SET_ADD` (23)

Add an IPC primitive to a wait set.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `wait_set_cap` | Wait set capability (Modify rights) |
| 1 | `source_cap` | Capability to an endpoint, signal, or event queue |
| 2 | `token` | Opaque u64 returned to the caller when this source is ready |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The `token` is chosen by the caller to identify the source in a subsequent
`SYS_WAIT_SET_WAIT` result. The kernel does not interpret it.

**Capability requirements:** `wait_set_cap` (Modify), `source_cap` (at least one of
Receive/Wait/Recv rights on the source).

**Errors:** `InvalidCapability`, `AccessDenied`, `OutOfMemory`.

---

### `SYS_WAIT_SET_REMOVE` (24)

Remove a previously added source from a wait set.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `wait_set_cap` | Wait set capability (Modify rights) |
| 1 | `source_cap` | Capability identifying the source to remove |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

**Capability requirements:** `wait_set_cap` (Modify).

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (source not in
this wait set).

---

### `SYS_WAIT_SET_WAIT` (25)

Block until any member of the wait set becomes ready.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `wait_set_cap` | Wait set capability (Wait rights) |

**Return:**

- `rax`/`a0`: 0 on success; `SyscallError` on failure
- `rdx`/`a1`: token of the ready source (valid on success)

Only one ready source is returned per call (wake-one semantics). If multiple sources
are ready simultaneously, subsequent calls return them without blocking.

**Capability requirement:** `wait_set_cap` must have Wait rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `Interrupted`.

---

## Interrupt Syscall

### `SYS_IRQ_ACK` (26)

Acknowledge a hardware interrupt line after handling. Re-enables the line at the
interrupt controller.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `irq_cap` | Interrupt capability for the line to acknowledge |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The kernel masks the interrupt line before delivering the notification to the driver
(see [docs/architecture.md](../../docs/architecture.md) — Driver Model). The driver
must call `SYS_IRQ_ACK` to re-enable the line. Calling `SYS_IRQ_ACK` without a
prior interrupt delivery has no effect.

**Capability requirement:** `irq_cap` must be a valid interrupt capability for the
specific line.

**Errors:** `InvalidCapability`, `AccessDenied`.

---

## Process Syscalls

### `SYS_CAP_CREATE_PROCESS` (27)

Create a new process with its own CSpace. The process has no threads initially;
threads are added by creating them with `SYS_CAP_CREATE_THREAD` using an address
space associated with the process.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Map rights) to assign to the process |
| 1 | `max_cspace_slots` | Maximum CSpace slots the new process may hold (ceiling) |

**Return:** `rax`/`a0`: new process capability (Control + Supervise rights) on success;
`SyscallError` on failure.

**Capability requirement:** `aspace_cap` must have Map rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (max_cspace_slots
is 0 or exceeds the system maximum), `OutOfMemory`.

---

### `SYS_CAP_INSERT` (28)

Insert a capability from the caller's CSpace into another process's CSpace. Used by
init to populate child process CSpaces before starting their threads.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `process_cap` | Process capability (Control rights) for the target process |
| 1 | `source_cap` | Capability to insert (moved, not copied) |
| 2 | `dest_slot` | Slot index in the target process's CSpace to insert into |
| 3 | `rights_mask` | Rights for the inserted capability (subset of source rights) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The source capability is moved: after a successful call, the caller no longer holds
it and the target process holds a capability to the same object with `rights_mask`
rights. The derivation tree position follows the slot (transfer semantics, not derive).

**Capability requirement:** `process_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied` (requested rights exceed source rights,
or dest_slot is already occupied), `InvalidArgument` (dest_slot out of range or
exceeds process ceiling), `OutOfMemory`.

---

### `SYS_PROCESS_KILL` (31)

Terminate another process externally. Equivalent to `SYS_PROCESS_EXIT` from the
process's own perspective, but issued by a holder of the process's Control capability.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `process_cap` | Process capability (Control rights) |
| 1 | `exit_code` | Exit code delivered to the process supervisor (if any) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

All threads in the target process are stopped, all capabilities are deleted, and
all resources are freed. If the process has a supervisor holding a Supervise
capability, a process-exit event is posted to its registered event queue.

**Capability requirement:** `process_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidState` (process already
exited).

---

### `SYS_PROCESS_SUPERVISE` (32)

Register an event queue to receive process lifecycle events (exit, fault) for the
target process.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `process_cap` | Process capability (Supervise rights) |
| 1 | `queue_cap` | Event queue capability (Post rights) to deliver events to |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

At most one supervisor event queue may be registered per process. A second call
replaces the previous registration. Pass a null capability descriptor in `queue_cap`
to deregister.

Events posted to the queue carry a word-sized payload encoding the event type and
exit code (format defined in the kernel ABI header).

**Capability requirements:** `process_cap` (Supervise), `queue_cap` (Post).

**Errors:** `InvalidCapability`, `AccessDenied`.

---

## Memory Syscalls (continued)

### `SYS_FRAME_SPLIT` (30)

Split a frame capability at a page boundary, producing two frame capabilities that
together cover the same physical range as the original. The original capability is
consumed.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `frame_cap` | Frame capability to split |
| 1 | `offset_pages` | Page offset within the frame at which to split |

**Return:**

- `rax`/`a0`: capability descriptor for the lower portion (pages 0..offset_pages)
  on success; `SyscallError` on failure
- `rdx`/`a1`: capability descriptor for the upper portion (pages offset_pages..end)
  on success

The original `frame_cap` is consumed by this call. Both halves inherit the same
rights as the original. The derivation tree treats both halves as children of the
original's position.

`offset_pages` must be in the range [1, frame_size_pages − 1].

**Errors:** `InvalidCapability`, `InvalidArgument` (offset out of range or frame
is already a single page), `OutOfMemory` (no free CSpace slot for second cap).

---

### `SYS_MMIO_MAP` (33)

Map an MMIO region capability into an address space. MMIO mappings use uncacheable
page attributes (`PAT` write-combine or uncacheable on x86-64; device-ordered on
RISC-V) rather than the default writeback caching.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Map rights) |
| 1 | `mmio_cap` | MMIO region capability (Map rights) |
| 2 | `virt` | Virtual address to map at (page-aligned) |
| 3 | `flags` | Mapping flags: readable, writable (not executable; MMIO is never XP) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

MMIO mappings are never executable. W^X enforcement still applies — the flags may
not set both writable and executable. The kernel forces the uncacheable attribute
regardless of the flags value; callers may not override this.

**Capability requirements:** `aspace_cap` (Map), `mmio_cap` (Map).

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (unaligned
`virt`, W^X violation, or non-canonical address), `AlignmentError`,
`AddressSpaceFull`.

---

### `SYS_DMA_GRANT` (35)

Program the IOMMU to permit a specific device to perform DMA to or from a physical
frame. The kernel records the grant in the IOMMU's device-to-domain mapping.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `process_cap` | Process capability (Control rights) of the owning process |
| 1 | `frame_cap` | Frame capability (Map rights) to grant DMA access to |
| 2 | `device_id` | Platform-specific device identifier (PCI BDF on x86-64) |
| 3 | `flags` | DMA direction: bit 0 = read, bit 1 = write |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The grant is revoked automatically when `frame_cap` is revoked or deleted. On
platforms without an IOMMU, this syscall succeeds without effect (DMA isolation
is not enforced).

**Capability requirements:** `process_cap` (Control), `frame_cap` (Map).

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (unknown
device_id or invalid flags).

---

## Interrupt Syscalls (continued)

### `SYS_IRQ_REGISTER` (29)

Register a signal to receive interrupt notifications for a hardware interrupt line.
When the interrupt fires, the kernel delivers it by ORing a notification bit into
the registered signal.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `irq_cap` | Interrupt capability for the line to register |
| 1 | `signal_cap` | Signal capability (Signal rights) to notify on interrupt |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

Only one signal may be registered per interrupt line at a time. A second call
replaces the previous registration. The kernel masks the interrupt line before
delivering the notification; the driver must call `SYS_IRQ_ACK` to re-enable it.

**Capability requirements:** `irq_cap` (valid interrupt capability), `signal_cap`
(Signal rights).

**Errors:** `InvalidCapability`, `AccessDenied`.

---

### `SYS_IOPB_GRANT` (34)

Grant a thread permission to access a range of x86 I/O ports directly via the
TSS I/O Permission Bitmap (IOPB). Ports not covered by the IOPB remain inaccessible
from userspace.

**x86-64 only.** On RISC-V this syscall returns `UnknownSyscall` immediately;
there is no port I/O concept.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Control rights) |
| 1 | `port_base` | First port in the range to grant (0–0xFFFF) |
| 2 | `port_count` | Number of ports to grant (1–65536; must not overflow 0xFFFF) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The grant is persistent for the lifetime of the thread. Revoking the thread
capability does not undo IOPB grants already applied; the thread is simply terminated
if the capability is revoked.

**Capability requirement:** `thread_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (port range
out of bounds), `UnknownSyscall` (RISC-V).

---

## Thread Syscalls (continued)

### `SYS_THREAD_SET_PRIORITY` (36)

Change a thread's scheduling priority after creation.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Control rights) |
| 1 | `priority` | New priority (1–`PRIORITY_MAX`) |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

Priority 0 (idle) and priority 31 (reserved) cannot be requested. The change takes
effect at the next scheduler invocation. If the thread is currently running, it
continues to run until preempted or blocked; no immediate preemption is forced.

**Capability requirement:** `thread_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (priority 0 or
priority 31).

---

### `SYS_THREAD_SET_AFFINITY` (37)

Set or change a thread's CPU affinity.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Control rights) |
| 1 | `cpu_id` | Target CPU ID, or `AFFINITY_ANY` (u32::MAX) to clear affinity |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

Setting a hard affinity (`cpu_id != AFFINITY_ANY`) prevents future migration by
the load balancer. The thread is migrated to the target CPU at the next scheduler
invocation if it is not already there. If `cpu_id` names an offline CPU, the call
fails with `InvalidArgument`.

**Capability requirement:** `thread_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (CPU offline or
out of range).

---

### `SYS_THREAD_READ_REGS` (38)

Read the full register state of a stopped thread into a caller-supplied buffer.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Observe rights) |
| 1 | `buf_ptr` | Pointer to buffer in caller's address space |
| 2 | `buf_size` | Size of the buffer in bytes |

**Return:** `rax`/`a0`: number of bytes written on success; `SyscallError` on failure.

The thread must be in `Stopped` state. The buffer receives an architecture-defined
register file structure (layout published in the kernel ABI headers). If `buf_size`
is smaller than the required size, the call fails with `InvalidArgument`.

**Capability requirement:** `thread_cap` must have Observe rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidState` (thread not stopped),
`InvalidArgument` (buffer too small or invalid pointer).

---

### `SYS_THREAD_WRITE_REGS` (39)

Write register state into a stopped thread from a caller-supplied buffer.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `thread_cap` | Thread capability (Control rights) |
| 1 | `buf_ptr` | Pointer to register file buffer in caller's address space |
| 2 | `buf_size` | Size of the buffer in bytes |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The thread must be in `Stopped` state. The kernel validates that the register values
are safe (e.g. the instruction pointer is in a canonical range; privilege bits cannot
be set). Writing a malformed register file returns `InvalidArgument`.

**Capability requirement:** `thread_cap` must have Control rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidState` (thread not stopped),
`InvalidArgument` (buffer wrong size, invalid pointer, or illegal register values).

---

## Address Space Syscall

### `SYS_ASPACE_QUERY` (40)

Query the mapping at a virtual address in an address space.

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `aspace_cap` | Address space capability (Read rights) |
| 1 | `virt` | Virtual address to query (page-aligned) |
| 2 | `buf_ptr` | Pointer to a buffer to receive mapping info |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The buffer receives an architecture-neutral mapping descriptor: physical address,
page size, and permission flags. If `virt` is not mapped, the call fails with
`InvalidArgument`. Layout is defined in the kernel ABI headers.

**Capability requirement:** `aspace_cap` must have Read rights.

**Errors:** `InvalidCapability`, `AccessDenied`, `InvalidArgument` (address not
mapped, unaligned, or non-canonical).

---

## IPC Buffer Syscall

### `SYS_IPC_BUFFER_SET` (41)

Register the per-thread IPC buffer page. This is the page the kernel uses for
extended IPC payloads (when `flags` bit 0 is set in `SYS_IPC_CALL` or
`SYS_IPC_REPLY`).

**Arguments:**

| # | Name | Description |
|---|---|---|
| 0 | `virt` | Page-aligned virtual address of the IPC buffer page |

**Return:** `rax`/`a0`: 0 on success; `SyscallError` on failure.

The page at `virt` must already be mapped in the calling thread's address space with
at least read+write permissions. The kernel records the address in the calling
thread's TCB. The page must remain mapped for the duration of any IPC that uses it;
if the page is unmapped when an extended IPC is attempted, the IPC syscall returns
`InvalidArgument`.

Calling `SYS_IPC_BUFFER_SET` again replaces the previous registration. Passing 0
deregisters the IPC buffer page (extended payloads will fail with `InvalidArgument`).

**Capability requirement:** None — acts on the calling thread implicitly.

**Errors:** `AlignmentError` (virt not page-aligned), `InvalidArgument` (page not
mapped or not writable; checked at registration time).

---

## Revocation Notes

### `SYS_CAP_REVOKE` targets the caller's own CSpace

`SYS_CAP_REVOKE` invalidates the capability in the caller's own CSpace slot and
all capabilities derived from it, across all processes. It cannot target a
capability in a remote process's CSpace directly — to revoke authority delegated to
another process, revoke the intermediary capability held in the caller's own CSpace.

### Delegating with the "derive twice" pattern

To delegate authority that can later be revoked without losing your own access:

```
1. Hold capability C (the original)
2. Derive C1 from C — you retain C1 as an intermediary
3. Derive C2 from C1 — C2 is the delegated capability
4. Transfer C2 to the child process via SYS_CAP_INSERT or IPC
5. To revoke: call SYS_CAP_REVOKE(C1) — destroys C1 and C2
   You still hold C with full rights.
```

This pattern works because revocation is subtree-local: revoking C1 removes C1 and
all its descendants (including C2) but leaves C and any other children of C intact.

---

## Atomicity and Preemption Guarantees

- **IPC message delivery is atomic.** A message either fully transfers (including all
  capability slots) or does not transfer at all. There is no partial delivery.

- **Capability operations are atomic.** Derivation, deletion, and revocation each
  complete fully before the syscall returns. A revocation that affects capabilities
  in other processes completes before `SYS_CAP_REVOKE` returns.

- **Memory mapping operations are atomic with respect to the address space.** After
  `SYS_MEM_MAP` or `SYS_MEM_UNMAP` returns, all CPUs see the updated mapping (TLB
  shootdowns complete before return).

- **Syscalls may be preempted.** Long-running operations (revocation traversal, SMP
  TLB shootdowns) may be interrupted by a higher-priority runnable thread. The kernel
  uses appropriate locks and re-checks state on resumption to ensure correctness.

- **Blocking syscalls are interruptible.** Any syscall that can block (`SYS_IPC_CALL`,
  `SYS_IPC_RECV`, `SYS_SIGNAL_WAIT`, `SYS_EVENT_RECV`, `SYS_WAIT_SET_WAIT`) returns
  `Interrupted` if the calling thread is stopped via `SYS_THREAD_STOP`.

---

## Constants

| Constant | Value | Meaning |
|---|---|---|
| `MSG_DATA_WORDS_MAX` | TBD (≥4) | Maximum data words per message |
| `MSG_CAP_SLOTS_MAX` | 4 | Maximum capabilities per message |
| `PRIORITY_MAX` | 30 | Maximum priority for userspace threads |
| `EVENT_QUEUE_MAX_CAPACITY` | 4096 | Maximum entries in an event queue |
| `BOOT_PROTOCOL_VERSION` | 1 | Expected version in `BootInfo.version` |

`MSG_DATA_WORDS_MAX` is fixed at implementation time. A value of 4–8 words balances
message capacity against syscall overhead. The exact value becomes stable ABI.
