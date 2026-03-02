# Seraph — AI Project Context

@../README.md

## Source of truth
- Design and architecture docs in `docs/` are authoritative.
- Module-specific design docs are referenced from each `<module>/README.md`.
- When implementation changes invalidate documentation, the documentation must always be updated.

## Global constraints
- Use `cargo xtask build`, `cargo xtask run`, and `cargo xtask clean` for building, running, and cleaning; do not invoke `cargo` directly for these tasks.
- Use `cargo xtask clean` in between any builds or runs of a different architecture.
- Always validate changes as much as possible, not just that it builds successfully, but also runs.
- Follow @docs/coding-standards.md at all times.
- Respect architectural invariants defined in the design docs.
