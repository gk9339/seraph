# xtask

Build task runner for Seraph. Invoked via `cargo xtask <command>`.

xtask is a standard Rust binary that runs on the host and will replace the
shell scripts in `scripts/` when the build requires disk image assembly,
multi-stage builds, or integration test orchestration.

Currently a stub. The shell scripts (`build.sh`, `clean.sh`, `run.sh`) remain
the primary build interface.

## Adding a command

1. Add a match arm in `src/main.rs`.
2. Implement the command as a function or submodule.
3. Update the usage string in the `None | Some("help") ...` arm.
