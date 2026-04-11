# Work Plan — Core Userspace Services

Informal tracking document. Not project documentation.

---

## Approach

Each tier: flesh out docs → implement → validate with a running milestone.
Move to the next tier once the milestone boots and works. Don't polish docs
for later tiers until the current tier is implemented — implementation will
reveal design issues that change the docs.

---

## Tier 1 — Process creation foundation

The base everything else is built on. Nothing can run without this.

### 1.1 abi/process-abi — flesh out
- [x] Finalise ProcessInfo struct layout (field sizes, alignment, page layout)
- [x] Finalise StartupInfo and main() signature
- [x] Decide CapDescriptor sharing strategy with init-protocol
      → CapDescriptor/CapType defined in process-abi; init-protocol re-exports
- [x] Define PROCESS_INFO_VADDR and stack layout constants
- [x] Write the crate code (types only, no logic)

### 1.2 shared/runtime — _start() stub
- [x] Create shared/runtime crate
- [x] Implement normal-process _start(): read ProcessInfo, construct StartupInfo, call main()
- [x] Init keeps its own _start() (not shared — only init uses that path)
- [x] Wire into build system (devmgr, procmgr link against it)

### 1.3 procmgr — docs then implement
- [x] Flesh out procmgr/docs/ipc-interface.md (CREATE_PROCESS spec)
- [x] Implement ELF loading from boot modules (shared/elf implemented)
- [x] Implement process creation: AddressSpace + CSpace + Thread + ProcessInfo page
- [x] Implement IPC endpoint: create_process
- [ ] Implement exit_process, query_process (deferred)
- [ ] Implement process teardown and resource reclamation (deferred)

### 1.4 Cross-cutting work completed
- [x] Kernel: mint boot module Frame caps, populate InitInfo module_frame_base/count
- [x] W^X relaxed from cap-creation-time to mapping-time enforcement;
      mem_map extended with prot_bits (arg5); PROT_READ/WRITE/EXEC constants
- [x] Memory pool Frame caps now carry MAP|WRITE|EXECUTE (root authority)
- [x] shared/elf: ELF64 parser (validate, load_segments iterator)
- [x] init: bootstrap procmgr via raw syscalls, send CREATE_PROCESS for devmgr
- [x] devmgr: temporary no-op stub using shared/runtime

**Milestone:** kernel boots → init starts procmgr → init requests procmgr to
create devmgr → devmgr starts and exits cleanly. ✓

---

## Tier 2 — Bootstrap path

### 2.1 procmgr frame pool — docs then implement
- [ ] Flesh out procmgr/docs/frame-management.md (pool design, allocation
      strategy, per-process accounting, reclamation on teardown)
- [ ] Implement frame pool: init delegates all remaining memory frame caps to
      procmgr at bootstrap; procmgr manages them as its internal physical
      memory authority (replaces the current fixed-batch delegation)
- [ ] Implement per-process resource tracking (frames allocated, reclaimed on
      exit/crash)
- [ ] Implement optional `request_frames` IPC operation for services that need
      runtime allocation beyond what they were delegated at spawn

procmgr is the natural owner of the userspace physical memory pool. It already
holds authority to create address spaces, CSpaces, and threads; the frame pool
completes its role as the unified process lifecycle manager. Services that need
frames at runtime (drivers for DMA, vfsd for buffer cache) request them via
procmgr IPC or receive a delegation at spawn time.

### 2.2 init — docs then implement
- [ ] Define compiled-in bootstrap order and cap delegation table
- [ ] Implement: start procmgr (direct, using shared/elf + shared/syscall)
      — partially done in Tier 1; extend with full cap delegation
- [ ] Implement: delegate all remaining memory frame caps to procmgr
- [ ] Implement: request early services via procmgr IPC (devmgr, svcmgr, etc.)
- [ ] Implement: derive-twice cap delegation for each service
- [ ] Implement: register services with svcmgr, then exit

**Milestone:** init bootstraps procmgr, devmgr, and svcmgr in sequence. All
three are running. procmgr holds the full memory pool. init exits.

---

## Tier 3 — Hardware access

### 3.1 devmgr — docs then implement
- [ ] Flesh out devmgr/docs/pci-enumeration.md
- [ ] Implement ACPI table parsing (x86-64) / Device Tree parsing (RISC-V)
- [ ] Implement PCI enumeration via ECAM MMIO
- [ ] Implement driver matching and spawn (request procmgr, delegate per-device caps)
- [ ] Implement device registry IPC endpoint

### 3.2 drivers/virtio — implement
- [ ] Implement virtio/core: transport init, virtqueue setup, descriptor chains
- [ ] Implement virtio/blk: block read/write over virtqueue, expose IPC endpoint
- [ ] Flesh out drivers/docs/ as needed during implementation

**Milestone:** devmgr enumerates PCI, discovers virtio-blk device, spawns
driver, driver exposes block IPC endpoint. Can read raw blocks.

---

## Tier 4 — Storage

### 4.1 vfsd — docs then implement
- [ ] Flesh out vfsd/docs/vfs-ipc-interface.md (open/read/write/close/stat/readdir)
- [ ] Flesh out fs/docs/fs-driver-protocol.md (vfsd-to-driver IPC)
- [ ] Implement mount table and path resolution
- [ ] Implement fs driver launch and lifecycle management
- [ ] Implement namespace IPC endpoint for applications

### 4.2 fs/fat — implement
- [ ] Implement FAT12/16/32 driver: BPB parsing, cluster chain traversal, directory reading
- [ ] Implement fs-driver-protocol responder (mount, open, read, stat, readdir)
- [ ] Write support can follow later (read-only first is fine)

**Milestone:** vfsd launches fat driver, mounts a FAT filesystem from
virtio-blk, serves file reads over IPC. Can read a file from disk end-to-end.

---

## Tier 5 — Resilience

### 5.1 svcmgr — docs then implement
- [ ] Flesh out svcmgr/docs/restart-protocol.md
- [ ] Implement service registration (receive from init during bootstrap)
- [ ] Implement health monitoring via thread lifecycle notifications
- [ ] Implement restart via procmgr IPC
- [ ] Implement procmgr fallback (raw syscall process creation)
- [ ] Implement restart policy enforcement (Always/OnFailure/Never, max retries)

**Milestone:** kill a service (e.g. devmgr), svcmgr detects the crash,
requests procmgr to restart it, devmgr comes back up with its capabilities
re-delegated.

---

## Cross-cutting work (as needed)

These aren't tiers — they come up naturally during implementation:

- **Kernel IPC** — if synchronous call/reply isn't fully wired yet, it blocks
  tier 1 (procmgr needs to receive IPC requests)
- **Kernel capability syscalls** — derive, transfer, revoke must work for
  init's cap delegation in tier 2
- **Kernel async notifications** — signals and event queues needed for
  svcmgr's crash detection in tier 5, and for interrupt delivery in tier 3
- **Boot module loading** — bootloader must load service ELF images and
  populate BootInfo.modules; kernel must pass module frame caps to init
