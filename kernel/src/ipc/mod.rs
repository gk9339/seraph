// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/ipc/mod.rs

//! IPC subsystem — synchronous endpoint messaging and signals.
//!
//! Implements the kernel side of:
//! - [`endpoint`]: blocking call/recv/reply using intrusive TCB queues.
//! - [`signal`]: bitmask-based asynchronous notification (OR bits / wait).
//! - [`message`]: the `Message` struct transferred through both mechanisms.
//!
//! # Adding IPC primitives
//! EventQueue and WaitSet are stubbed for future phases. Add their modules
//! here following the same pattern as `endpoint` and `signal`.

pub mod endpoint;
pub mod message;
pub mod signal;

// EventQueue and WaitSet: deferred to post-Phase 9.
// pub mod event_queue;
// pub mod wait_set;
