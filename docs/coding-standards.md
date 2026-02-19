# Coding Standards

These standards apply across the entire Seraph codebase. All contributors (and AI assistants)
must read this document before writing or reviewing code. Where a rule conflicts with a
language's own enforced conventions, the language wins — this document covers everything else.

---

## Philosophy

Rules here exist to serve two goals: **correctness** and **readability**. A rule that
makes code harder to understand or more error-prone should be challenged. Rules are not
cargo-culted from other projects — each has a reason, documented below it where non-obvious.

Where there is no rule, prefer the most explicit and least surprising option.

---

## Formatting

### Indentation

4 spaces. No tabs, except where the tooling requires them (Makefiles, certain shell heredocs).
Never mix tabs and spaces in the same file.

### Line Length

100 columns maximum, for all languages. This accommodates comfortable side-by-side editing
on a laptop without forcing aggressive wrapping on ordinary code.

### Brace Style

Allman style throughout — opening brace on its own line, always. No exceptions for
single-statement bodies; braces are never omitted.

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

For `if-else` chains, `else` follows the closing brace on the same line to avoid
excessive vertical spacing:

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

The `*` attaches to the type, not the variable name. A pointer is a property of the
type, not the identifier.

```c
// Correct
int* ptr;
char* name;

// Wrong
int *ptr;
char *name;
```

When declaring multiple pointers on one line, use a separate declaration per variable
to avoid ambiguity:

```c
// Correct
int* a;
int* b;

// Wrong — only 'a' is a pointer here, misleading with type-attached style
int* a, b;
```

---

## Naming

### General Rules (All Languages)

- `snake_case` for variables, functions, and modules
- `SCREAMING_SNAKE_CASE` for constants and macros
- Names should be descriptive enough to need no comment. If a name requires a comment
  to explain what it refers to, rename it instead.
- Abbreviations are acceptable only when universally understood in context
  (e.g. `addr`, `buf`, `len`, `idx`). Avoid novel abbreviations.

### Rust

Follow Rust language conventions:

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

Struct and enum tags (before typedef) use plain `snake_case` without `_t`:

```c
typedef struct process
{
    pid_t pid;
    char* name;
} process_t;
```

### Assembly

Follow the target architecture's register naming conventions. Labels use `snake_case`.
Global symbols exported to other translation units are prefixed with the component name
to avoid collisions (e.g. `kernel_entry`, `boot_gdt`).

---

## Function Design

- Functions should do one thing. If a function needs a comment to separate it into
  phases, it should be split.
- Prefer functions under 50 lines. Functions over 100 lines require strong justification.
- Prefer many small, named functions over inline complexity. A named function is
  self-documenting; a 40-line `if` tree is not.
- Avoid boolean parameters that alter function behaviour — prefer separate functions or
  an explicit enum.

---

## Error Handling

### Rust

- All fallible operations return `Result`. Callers handle errors explicitly.
- `unwrap()` and `expect()` are forbidden in production code paths. They are permitted
  in tests and in `const` contexts where the compiler guarantees the value is `Some`/`Ok`.
- Do not use `panic!` in production code paths. A kernel panic is a last resort for
  unrecoverable states only (corrupted kernel structures, failed critical assertions),
  not a substitute for error handling.
- Error types should be defined per-subsystem and carry enough context for the caller
  to make a decision without inspecting internal state.

### C

- Functions that can fail return a status code or a pointer with a sentinel error value.
  Error paths are always documented in the function's comment.
- Do not silently discard return values from fallible functions. If a return value is
  intentionally ignored, document why.

---

## Assertions

Assertions communicate invariants — things that *must* be true for the program to be
correct. They are not error handling.

- `debug_assert!` (Rust) / `assert()` in debug builds (C): use liberally for internal
  invariants. These are optimised out in release builds and exist to catch logic errors
  during development.
- `assert!` (Rust) / unconditional `assert()` (C): use for invariants that, if violated,
  indicate a fundamental and unrecoverable correctness failure. These remain in release
  builds. Use sparingly and only where the cost is justified.
- Assertions are not input validation. Never assert on values that originate from
  outside the kernel (user input, hardware, boot info). Return an error instead.

---

## Unsafe Code

Unsafe is sometimes necessary. It must always be contained and justified.

- Unsafe blocks must be as small as possible — wrap only the lines that require it.
- Every unsafe block must be preceded by a `// SAFETY:` comment explaining why the
  operation is sound: what invariants hold, what has been checked, and why this cannot
  be expressed in safe Rust.
- Unsafe code must never be used to work around a design problem. If safe Rust cannot
  express something cleanly, reconsider the design before reaching for unsafe.
- Public functions that are `unsafe` must document their safety contract in their
  rustdoc comment under a `# Safety` heading.

```rust
// SAFETY: `ptr` is non-null and correctly aligned, and we hold the exclusive
// lock on this region for the duration of this call.
let value = unsafe { ptr.read() };
```

---

## Memory Allocation

- Allocation can fail. All allocation paths must handle failure explicitly — no silent
  OOM conditions.
- In the kernel, prefer static or pool allocation over dynamic allocation on hot paths.
- Document why dynamic allocation is acceptable at each site where it is used.
- Never allocate inside interrupt handlers.

---

## Concurrency

- Shared mutable state must be protected by an explicit synchronisation primitive.
  The type system should enforce this — use `Mutex<T>` rather than a bare `T` with a
  separate lock.
- Prefer message passing over shared memory for communication between components.
  Shared memory should be a deliberate optimisation, not the default.
- Lock ordering must be documented and consistent to prevent deadlock. If acquiring
  multiple locks, always acquire them in the documented order.
- Avoid holding locks across IPC calls or any operation that may block.

---

## Documentation

- All public APIs must have rustdoc comments. The comment must describe what the
  function does, its arguments, return value, and any errors it can return.
- Comments explain *why*, not *what*. Code that is clear enough to need no explanation
  should not have one. Code whose logic is non-obvious must explain the reasoning.
- Do not leave TODO comments without context. A TODO must state what needs doing and why
  it was deferred, not just that something is incomplete.
- Architecture decisions that are not obvious from the code should be documented in the
  relevant `docs/` file, not only in inline comments.

---

## Architecture-Specific Code

- All architecture-specific behaviour must be behind a trait or module boundary.
  No `#[cfg(target_arch = "x86_64")]` blocks in architecture-neutral code.
- Inline assembly must be isolated to dedicated functions or modules. It must never
  be inlined mid-function in code that also contains logic.
- Every block of inline assembly must carry a comment explaining what it does,
  what registers it clobbers, and what constraints it assumes.
- When adding support for a new architecture, the existing architecture's implementation
  serves as the reference. Do not diverge from the interface contract without updating
  both implementations.

---

## Testing

### Philosophy

Tests are not optional. Every module has tests. An untested code path is an unknown
code path — treat it as broken until proven otherwise. Tests are written alongside
the code they cover, not after the fact.

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
- Tests are independent — no test depends on another's side effects or order.
- Tests are deterministic — no randomness, no timing, no external state.

### What Must Be Tested

- Every public function: at least one test for the primary success path, at least
  one for each distinct failure mode.
- Boundary conditions: empty input, maximum-size input, off-by-one cases.
- Every `Result::Err` variant a function can return must be exercised by a test.
- Any module containing `unsafe` blocks must have tests exercising the safe
  wrapper to confirm the invariants hold under normal use.

### What Should Not Be Tested

- Private implementation details not visible through the public interface.
- Trivial getters and setters with no logic.
- Code generated by `#[derive]` macros, unless custom logic is attached.

### Kernel Testing Strategy

Kernel code is `no_std` and cannot use the standard Rust test harness directly
when compiled for a custom target. Two strategies are used:

**Host unit tests** — Pure algorithmic modules (buddy allocator, slab allocator,
capability derivation tree, scheduler run queues) are designed so that hardware
dependencies are behind trait boundaries. The kernel crate has a `lib` target
(`src/lib.rs`) compiled with `#![cfg_attr(not(test), no_std)]`, which lifts the
`no_std` restriction when building for the host during `cargo test`. Tests in
these modules run on the host with the standard test harness:

```
cargo test -p seraph-kernel
```

**QEMU integration tests** — Code that requires real hardware interaction (page
table manipulation, interrupt handling, context switching) is tested under QEMU
using a custom test harness. The kernel is compiled in a test configuration that
runs test functions sequentially and reports results via the serial port. QEMU
exits with a known status code. This harness will be implemented when arch code
is written.

### Assertions in Tests

`assert!`, `assert_eq!`, `assert_ne!`, `unwrap()`, and `expect()` are all
permitted in test code. These are the only contexts where `unwrap()` and
`expect()` are allowed. Test panics are the intended failure mechanism.

### Running Tests

```sh
# Host unit tests (runs immediately, no QEMU required)
cargo test -p seraph-kernel

# QEMU integration tests (not yet implemented)
./build.sh --arch x86_64 --test
```
