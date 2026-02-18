# libc

C standard library for Seraph userspace. Provides the C runtime and standard
library functions for components written in C or for C-compatible FFI. Implements
Seraph's native syscall ABI rather than POSIX — there is no fork, no signals, and
no POSIX process model.

Rust components use the kernel syscall interface directly via the kernel ABI crate;
libc is for C code that needs a standard environment.
