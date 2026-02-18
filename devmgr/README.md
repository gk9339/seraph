# devmgr

Userspace device manager. Launched by init early in the service startup sequence,
devmgr is responsible for platform enumeration and driver binding.

devmgr receives platform resource capabilities (MMIO regions, interrupt lines,
I/O port ranges, IOMMU units) and read-only access to firmware tables (ACPI RSDP
on x86-64, Device Tree blob on RISC-V) from init. It then:

- Parses firmware tables in userspace to build a complete picture of the platform
- Enumerates PCI devices via the ECAM MMIO capability
- Spawns driver processes from `drivers/` and delegates per-device capabilities
- Exposes a device registry IPC service for vfs, net, and other services to query
- Handles hotplug events on platforms that support them

devmgr is privileged but not omnipotent — it holds only what init delegates to it,
and init retains intermediary capabilities so that devmgr's authority can be revoked
and re-delegated on restart.

Full design, including the DMA safety model and the security boundary, is documented
in [docs/device-management.md](../docs/device-management.md).
