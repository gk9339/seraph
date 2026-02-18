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
