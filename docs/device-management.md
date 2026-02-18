# Device Management

## Overview

Device management in Seraph is a userspace concern. The kernel does not parse
firmware tables, enumerate buses, or bind drivers. Its role is limited to minting
initial capabilities from boot-provided resource descriptors and enforcing access
control on hardware regions. All enumeration, binding, and policy live in `devmgr`,
a privileged userspace process launched by init.

---

## Boot-Provided Resource Descriptors

The bootloader parses platform firmware tables (ACPI on x86-64; Device Tree on
RISC-V) and extracts structured resource descriptors before jumping to the kernel.
These descriptors are passed to the kernel in the `platform_resources` field of
`BootInfo` (see [boot-protocol.md](boot-protocol.md)).

The kernel does not parse ACPI or Device Tree itself. It consumes the already-parsed
`PlatformResource` entries and mints capabilities from them during Phase 7 of
initialization. Each entry describes one discrete hardware resource: an MMIO range,
an interrupt line, a PCI ECAM window, an I/O port range, an IOMMU unit, or a
platform firmware table region.

This design keeps firmware parsing code out of the kernel's trusted computing base.
Bugs in ACPI or Device Tree parsing cannot corrupt the kernel — they can only produce
incorrect resource descriptors, which init and devmgr observe and can reject.

---

## Raw Firmware Passthrough

The `acpi_rsdp` and `device_tree` fields in `BootInfo` are passed through to
userspace as opaque physical addresses. The kernel creates read-only frame
capabilities for these regions so that devmgr (or any other process init authorises)
can parse them directly.

This enables userspace to perform its own firmware interpretation without requiring
any firmware parsing code in the kernel. The kernel treats these regions as opaque
byte ranges.

---

## devmgr: Userspace Device Manager

`devmgr` is a privileged userspace process launched by init early in the service
startup sequence. It is the single point responsible for platform enumeration and
driver binding in a running system.

### What devmgr receives from init

At startup, devmgr receives from init (via `SYS_CAP_INSERT`):

- **Platform resource capabilities** — one capability per `PlatformResource` entry
  in the initial CSpace: MMIO region caps, interrupt caps, IoPortRange caps, and
  IOMMU unit caps.
- **Firmware table capabilities** — read-only frame caps for the ACPI RSDP and/or
  Device Tree blob, enabling devmgr to parse them in userspace.
- **SchedControl capability** — so devmgr can assign slightly elevated priorities
  to latency-sensitive driver threads if needed.

Init retains its own copies (derived from the boot capabilities) so that it can
revoke devmgr's authority if devmgr crashes or is restarted.

### What devmgr does

1. **Parse firmware tables** — devmgr walks the ACPI tables or Device Tree blob
   to build a complete picture of the platform's device topology beyond what the
   bootloader extracted. It resolves interrupt routing, identifies device power
   domains, and records the full PCI hierarchy.

2. **Enumerate PCI** — devmgr maps the ECAM region (via the PciEcam capability
   from init) and reads the PCI configuration space to discover all devices on all
   buses. It identifies device classes, vendor IDs, BARs, and interrupt assignments.

3. **Bind drivers** — for each discovered device, devmgr consults a driver registry
   to identify the appropriate driver binary. It:
   - Spawns a new process for the driver
   - Delegates per-device capabilities: the MMIO cap for the device's BARs, the
     interrupt cap for the device's IRQ lines, and (if the device is a DMA master)
     a DMA grant to its IOMMU domain
   - Passes the driver process's endpoint to the appropriate service (e.g. passes
     a storage driver endpoint to the VFS server)

4. **Expose a device registry** — devmgr maintains an IPC service that other
   userspace processes can query to discover device capabilities. This is the
   mechanism by which higher-level services find their devices after boot.

5. **Handle hotplug** — on platforms that support it, devmgr receives hotplug
   notifications (via interrupt or firmware callbacks routed through the kernel
   as IPC notifications) and dynamically spawns or terminates driver processes.

### Security boundary

devmgr is privileged but not omnipotent. It holds only the capabilities delegated
to it by init — it cannot access hardware it was not given access to. Its authority
is revocable: if init kills devmgr (e.g. after a crash), all capabilities devmgr
has delegated to driver processes can be revoked by revoking init's intermediary
capabilities, and devmgr can be restarted with a fresh capability set.

A compromised devmgr can misdelegate or misuse the capabilities it holds, but it
cannot escalate beyond the authority init gave it. Kernel resources it was never
given (e.g. memory regions belonging to another process) remain inaccessible.

---

## DMA Safety Model

DMA access in Seraph operates in one of two explicit modes:

**IOMMU-isolated (safe):** When an IOMMU is present and active, `SYS_DMA_GRANT`
programs the IOMMU to confine a device's DMA to the specified physical frames.
A driver cannot DMA outside its authorised regions even if its process is compromised.
This is the expected mode on modern x86-64 hardware and on RISC-V platforms that
implement the IOMMU extension.

**DMA-unsafe:** When no IOMMU is present (or when the IOMMU has not been configured
for a device), unconfined DMA is physically possible. In this mode, `SYS_DMA_GRANT`
requires the caller to pass `FLAG_DMA_UNSAFE` (bit 2 in the flags argument),
explicitly acknowledging that DMA isolation is not enforced. Without this flag,
`SYS_DMA_GRANT` returns `DmaUnsafe`.

devmgr is responsible for querying `SYS_SYSTEM_INFO(DMA_MODE)` at startup to
determine which mode is active, and for making the policy decision of whether to
proceed with unsafe DMA for a given device. devmgr may choose to:
- Refuse to bind a driver that requires DMA on a platform without IOMMU protection.
- Bind the driver but pass `FLAG_DMA_UNSAFE` and warn the operator.
- Bind the driver in a restricted mode that avoids DMA entirely.

This decision is a userspace policy choice. The kernel only enforces the boundary
between modes — it does not silently degrade.

---

## IommuUnit Resources

`IommuUnit` entries in `PlatformResources` describe the register base and scope of
one IOMMU. devmgr receives an MMIO capability for each IOMMU's register range and
is responsible for configuring domain mappings before allowing any device under that
IOMMU's scope to perform DMA.

The kernel does not configure IOMMU domains itself. It relies on devmgr to program
the IOMMU before issuing `SYS_DMA_GRANT` calls. If devmgr has not configured the
IOMMU and a driver calls `SYS_DMA_GRANT`, the kernel programs the IOMMU grant at
that point using whatever domain state devmgr has established — or returns an error
if the IOMMU state is inconsistent.

---

## Relationship to Other Services

```
init
 ├── devmgr  (platform caps + firmware table caps)
 │    ├── driver/ethernet  (MMIO cap, IRQ cap, DMA grant)
 │    ├── driver/nvme      (MMIO cap, IRQ cap, DMA grant)
 │    └── driver/usb-hcd   (MMIO cap, IRQ cap, DMA grant)
 ├── vfs  (receives storage endpoint from devmgr)
 ├── net  (receives network endpoint from devmgr)
 └── ...
```

devmgr is not a dependency of vfs or net directly — those services receive device
endpoints after devmgr has completed initial binding. The dependency ordering is
managed by init's service supervision graph.
