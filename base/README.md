# base

General-purpose userspace applications and utilities: terminal emulator, shell
(gksh), text editor, coreutils equivalents, and network utilities.

These are applications, not services — they have no special privileges beyond what
their capabilities grant. They interact with the system through the IPC interfaces
exposed by vfs, net, and other services.
