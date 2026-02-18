# drivers

Userspace device drivers. Each driver is a separate process with its own isolated
address space, receiving only the capabilities for the specific device it manages:
MMIO region, interrupt line, DMA grant, and (on x86) IoPortRange if needed.

Drivers are spawned and supervised by devmgr, which binds drivers to discovered
devices and delegates per-device capabilities. No driver code runs in kernel space.

See [docs/device-management.md](../docs/device-management.md) for how drivers are
discovered, bound, and revoked.
