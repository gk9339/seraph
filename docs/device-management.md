# Device Management

Device management is a userspace concern. The kernel mints initial capabilities from
boot-provided resource descriptors and enforces hardware access control. All
enumeration, binding, and policy live in `devmgr`.

---

## Boot-Provided Resource Descriptors (summary — [boot-protocol.md](boot-protocol.md))

The kernel consumes `PlatformResource` entries from `BootInfo.platform_resources`
and mints capabilities from them during Phase 7 of initialization. Firmware parsing
is outside the kernel's TCB.

See [boot-protocol.md](boot-protocol.md) for the `PlatformResource` type and field
definitions.

---

## Raw Firmware Passthrough

The `acpi_rsdp` and `device_tree` fields in `BootInfo` are passed through to
userspace as opaque physical addresses. The kernel creates read-only frame
capabilities for these regions so that devmgr (or any other process init authorises)
can parse them directly.

The kernel treats these regions as opaque byte ranges.

---

## devmgr: Userspace Device Manager

`devmgr` is a privileged userspace process launched during bootstrap (started by init
via procmgr). It is the single point responsible for platform enumeration and driver
binding in a running system.

### What devmgr receives from init

At startup, devmgr receives from init (via `SYS_CAP_INSERT`):

- **Platform resource capabilities** — one per `PlatformResource` entry: MMIO,
  interrupt, IoPortRange, and IOMMU unit caps.
- **Firmware table capabilities** — read-only frame caps for the ACPI RSDP and/or
  Device Tree blob.
- **SchedControl capability** — for assigning elevated priorities to latency-sensitive
  driver threads.

Init retains derived copies to revoke devmgr's authority if devmgr crashes.

### What devmgr does

1. **Parse firmware tables** — walks ACPI or Device Tree to resolve interrupt
   routing, power domains, and the full PCI hierarchy.

2. **Enumerate PCI** — maps the ECAM region and reads configuration space to
   discover all devices, BARs, and interrupt assignments.

3. **Bind drivers** — for each device, spawns a driver process, delegates
   per-device capabilities (MMIO, interrupt, optionally DMA), and routes the
   driver's endpoint to the consuming service.

4. **Expose a device registry** — maintains an IPC service for querying device
   capabilities.

5. **Handle hotplug** — on supported platforms, receives hotplug notifications
   and dynamically spawns or terminates driver processes.

### Security boundary

devmgr holds only the capabilities delegated to it by init. Its authority is
revocable; init can kill devmgr and restart it with a fresh capability set.

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
 ├── vfsd  (receives storage endpoint from devmgr)
 ├── netd  (receives network endpoint from devmgr)
 └── ...
```

devmgr is not a dependency of vfsd or netd directly — those services receive device
endpoints after devmgr has completed initial binding. The dependency ordering is
managed by init's bootstrap sequence (for early boot) and svcmgr (for restarts).

---

## Summarized By

[Architecture Overview](architecture.md)
