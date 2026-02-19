# Coding Standards

These standards apply across the entire Seraph codebase. All contributors (and AI assistants)
must read this document before writing or reviewing code. Where a rule conflicts with a
language's own enforced conventions, the language wins.

---

## Philosophy

Rules exist to serve **correctness** and **readability**. A rule that undermines either should
be challenged. Where there is no rule, prefer the most explicit and least surprising option.

---

## File Headers

Every source file opens with a license block, a path line, and a brief description. Author
names and dates are not tracked in file headers — version control handles that.

### Structure

Elements appear in this order, each separated by a blank line:

1. **License block** — SPDX identifier, then copyright line(s)
2. **Path** — repository-relative path to this file
3. **Description** — brief summary of the file's purpose

For shell scripts with a shebang, the shebang precedes the license block on line 1.

### License Block

```
SPDX-License-Identifier: GPL-2.0-only
Copyright (C) <year> <name> <email>
```

Comment syntax follows the file type: `//` for Rust and assembly, `#` for shell. Files using
block comments use the `/* * ... */` style throughout, headers included.

### Description in Rust Files

Rust files use `//!` inner doc comments for the description rather than plain comments. These
are the module's rustdoc entry. The first `//!` line is the short summary shown in module index
views; additional paragraphs, separated by a blank `//!` line, appear only on the module's own
page. Crate-level attributes (`#![no_std]`, `#![cfg_attr(...)]`) follow the `//!` block.

### Third-Party Attribution

Files derived from third-party sources list the original copyright before the project copyright,
with a note identifying the source. Copyright lines stack chronologically. Add your own copyright
when you have made meaningful original contributions to the file; omit it for near-verbatim copies.

---

## Formatting

### Indentation

4 spaces. No tabs, except where tooling requires them (Makefiles, certain shell heredocs).
Never mix tabs and spaces in the same file.

### Line Length

100 columns maximum, for all languages.

### Brace Style

Allman style throughout — opening brace on its own line, always. Braces are never omitted,
even for single-statement bodies.

```rust
fn do_something(x: u32) -> Result<(), Error>
{
    if x == 0
    {
        return Err(Error::InvalidInput);
    }

    Ok(())
}
```

For `if-else` chains, `else` follows the closing brace on the same line:

```rust
if condition
{
    something();
} else
{
    something_else();
}
```

The same applies to `else if`, `} while`, and similar continuations.

### C: Pointer Declarations

The `*` attaches to the type, not the variable name:

```c
int* ptr;       // correct
int *ptr;       // wrong

int* a;         // correct — one declaration per pointer variable
int* b;
int* a, b;      // wrong — only 'a' is a pointer here
```

---

## Naming

### General Rules (All Languages)

- `snake_case` for variables, functions, and modules
- `SCREAMING_SNAKE_CASE` for constants and macros
- Names should be self-describing. If a name requires a comment to explain what it refers to,
  rename it instead.
- Abbreviations only when universally understood in context (`addr`, `buf`, `len`, `idx`).
  Avoid novel abbreviations.

### Rust

| Item | Convention | Example |
|---|---|---|
| Variables, functions, methods | `snake_case` | `frame_count` |
| Modules | `snake_case` | `memory::paging` |
| Types (structs, enums, unions) | `PascalCase` | `PageTable` |
| Traits | `PascalCase` | `FrameAllocator` |
| Constants and statics | `SCREAMING_SNAKE_CASE` | `MAX_ORDER` |
| Enum variants | `PascalCase` | `Error::OutOfMemory` |

### C

| Item | Convention | Example |
|---|---|---|
| Variables and functions | `snake_case` | `map_region` |
| Typedef'd structs and enums | `snake_case_t` | `process_t` |
| Macros and constants | `SCREAMING_SNAKE_CASE` | `PAGE_SIZE` |

Struct and enum tags use plain `snake_case` without `_t`:

```c
typedef struct process
{
    pid_t pid;
    char* name;
} process_t;
```

### Assembly

Follow the target architecture's register naming conventions. Labels use `snake_case`.
Global symbols are prefixed with the component name to avoid collisions
(e.g. `kernel_entry`, `boot_gdt`).

---

## Function Design

- Functions do one thing. If a function needs a comment to separate phases, split it.
- Prefer functions under 50 lines. Functions over 100 lines require strong justification.
- Prefer many small, named functions over inline complexity.
- Avoid boolean parameters that alter behaviour — prefer separate functions or an explicit enum.

---

## Error Handling

### Rust

- All fallible operations return `Result`. Callers handle errors explicitly.
- `unwrap()` and `expect()` are forbidden in production code paths. Permitted in tests and
  in `const` contexts where the value is statically guaranteed.
- Do not use `panic!` in production code. A kernel panic is a last resort for unrecoverable
  states only, not a substitute for error handling.
- Error types are defined per-subsystem and carry enough context for the caller to decide
  without inspecting internal state.

### C

- Functions that can fail return a status code or a sentinel error value. Error paths are
  documented in the function comment.
- Do not silently discard return values from fallible functions. If intentionally ignored,
  document why.

---

## Assertions

Assertions communicate invariants — conditions that must hold for the program to be correct.
They are not error handling.

- `debug_assert!` / `assert()` in debug builds: use liberally for internal invariants.
  Removed in release builds.
- `assert!` / unconditional `assert()`: use only for invariants whose violation indicates an
  unrecoverable correctness failure. Remain in release builds; use sparingly.
- Never assert on external values (user input, hardware registers, boot info).
  Return an error instead.

---

## Unsafe Code

- Unsafe blocks must be as small as possible — wrap only the lines that require it.
- Every unsafe block must be preceded by a `// SAFETY:` comment explaining why the operation
  is sound: what invariants hold, what has been checked, and why safe Rust cannot express it.
- Never use unsafe to work around a design problem — reconsider the design first.
- `unsafe fn` must document their safety contract under a `# Safety` rustdoc heading.

```rust
// SAFETY: `ptr` is non-null and correctly aligned, and we hold the exclusive
// lock on this region for the duration of this call.
let value = unsafe { ptr.read() };
```

---

## Memory Allocation

- Allocation can fail. All allocation paths handle failure explicitly — no silent OOM.
- In the kernel, prefer static or pool allocation on hot paths. Document why dynamic
  allocation is acceptable at each site where it appears.
- Never allocate inside interrupt handlers.

---

## Concurrency

- Shared mutable state must be protected by an explicit synchronisation primitive. Use
  `Mutex<T>` rather than a bare `T` with a separate lock — the type system should enforce
  the invariant.
- Prefer message passing over shared memory; shared memory is a deliberate optimisation,
  not the default.
- Lock ordering must be documented and consistent. When acquiring multiple locks, always
  take them in the documented order.
- Never hold a lock across an IPC call or any operation that may block.

---

## Documentation

- All public APIs must have rustdoc comments covering behaviour, arguments, return value,
  and all error variants.
- Comments explain *why*, not *what*. Self-evident code needs no comment; non-obvious logic
  must explain its reasoning.
- TODO comments must state what needs doing and why it was deferred.
- Architecture decisions not obvious from the code belong in the relevant `docs/` file,
  not only in inline comments.

---

## Architecture-Specific Code

- All architecture-specific behaviour must be behind a trait or module boundary.
  No `#[cfg(target_arch)]` blocks in architecture-neutral code.
- Inline assembly must be isolated to dedicated functions or modules; never inlined
  alongside logic.
- Every inline assembly block must comment what it does, what registers it clobbers,
  and what constraints it assumes.
- When adding a new architecture, do not diverge from the interface contract without
  updating both implementations.

---

## Testing

### Philosophy

Tests are not optional. Every module has tests, written alongside the code they cover.
An untested path is an unknown path.

### Unit Tests

Unit tests live in a `#[cfg(test)]` module at the bottom of each source file:

```rust
#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn alloc_fails_when_no_regions_added()
    {
        let mut alloc = BuddyAllocator::new(10);
        assert_eq!(alloc.alloc(0), None);
    }
}
```

Rules:
- One logical assertion per test function.
- Test names read as a sentence describing expected behaviour.
- Tests are independent and order-independent.
- Tests are deterministic — no randomness, no timing, no external state.

### What Must Be Tested

- Every public function: at least one success-path test, at least one test per failure mode.
- Boundary conditions: empty input, maximum-size input, off-by-one cases.
- Every `Result::Err` variant a function can return must be exercised.
- Modules containing `unsafe` blocks must have tests confirming the safe wrapper upholds
  its invariants under normal use.

### What Should Not Be Tested

- Private implementation details not visible through the public interface.
- Trivial getters and setters with no logic.
- Code generated by `#[derive]`, unless custom logic is attached.

### Kernel Testing Strategy

**Host unit tests** — Pure algorithmic modules (buddy allocator, slab allocator, capability
tree, scheduler run queues) keep hardware dependencies behind trait boundaries. The kernel's
`lib` target uses `#![cfg_attr(not(test), no_std)]`, allowing `cargo test -p seraph-kernel`
to run these modules on the host under the standard test harness.

**QEMU integration tests** — Code requiring real hardware (page tables, interrupts, context
switching) is tested under QEMU with a custom harness that runs tests sequentially and reports
results over serial. This harness will be implemented when arch code is written.

### Assertions in Tests

`assert!`, `assert_eq!`, `assert_ne!`, `unwrap()`, and `expect()` are all permitted in test
code — the only contexts where `unwrap()` and `expect()` are allowed.

### Running Tests

```sh
cargo test -p seraph-kernel          # host unit tests
./build.sh --arch x86_64 --test      # QEMU integration tests (not yet implemented)
```
