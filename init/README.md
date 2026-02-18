# init

Service manager and PID 1. The first userspace process, spawned directly by the
kernel at the end of boot. init receives the full set of initial capabilities
(hardware regions, interrupt lines, firmware table access, SchedControl) and is
responsible for delegating appropriate subsets to each service it starts.

init reads a boot configuration, starts system services in dependency order
(devmgr first, then vfs, net, and drivers), and supervises them — restarting
services that crash, and revoking their capabilities when they terminate.

Relationship to other components: init is the trust root for all userspace
authority. No service receives more capability than init explicitly delegates.
