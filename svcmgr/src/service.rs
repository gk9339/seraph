// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// svcmgr/src/service.rs

//! Service table and capability classification for svcmgr.
//!
//! Defines the `ServiceEntry` struct used to track monitored services and the
//! `SvcmgrCaps` struct for well-known capabilities discovered at startup.

use ipc::{LOG_ENDPOINT_SENTINEL, PROCMGR_ENDPOINT_SENTINEL, SERVICE_ENDPOINT_SENTINEL};
use process_abi::{CapType, StartupInfo};

/// Maximum number of monitored services.
pub const MAX_SERVICES: usize = 16;

/// Maximum restart attempts before marking degraded.
pub const MAX_RESTARTS: u32 = 5;

/// Restart policy: restart unconditionally on any exit.
pub const POLICY_ALWAYS: u8 = 0;

/// Restart policy: restart only on fault (nonzero exit reason).
pub const POLICY_ON_FAILURE: u8 = 1;

/// Criticality: crash of this service is fatal — halt the system.
pub const CRITICALITY_FATAL: u8 = 0;

/// Criticality: crash can be handled by restart policy.
pub const CRITICALITY_NORMAL: u8 = 1;

/// VA for mapping child `ProcessInfo` frames during cap injection on restart.
pub const CHILD_PI_VA: u64 = 0x0000_0002_0000_0000; // 8 GiB

// ── Service table ───────────────────────────────────────────────────────────

/// A monitored service entry in svcmgr's service table.
pub struct ServiceEntry
{
    /// Service name, packed into a fixed-size buffer.
    pub name: [u8; 32],
    /// Length of the service name in bytes.
    pub name_len: u8,
    /// Capability slot for the service's thread.
    pub thread_cap: u32,
    /// Capability slot for the service's boot module (used for restart).
    pub module_cap: u32,
    /// Capability slot for the service's log endpoint.
    pub log_ep_cap: u32,
    /// Restart policy (`POLICY_ALWAYS`, `POLICY_ON_FAILURE`, etc.).
    pub restart_policy: u8,
    /// Criticality level (`CRITICALITY_FATAL`, `CRITICALITY_NORMAL`).
    pub criticality: u8,
    /// Capability slot for the death-notification event queue.
    pub event_queue_cap: u32,
    /// Number of restart attempts so far.
    pub restart_count: u32,
    /// Whether this service is currently active.
    pub active: bool,
}

impl ServiceEntry
{
    /// Create an empty (inactive) service entry.
    pub const fn empty() -> Self
    {
        Self {
            name: [0; 32],
            name_len: 0,
            thread_cap: 0,
            module_cap: 0,
            log_ep_cap: 0,
            restart_policy: 0,
            criticality: 0,
            event_queue_cap: 0,
            restart_count: 0,
            active: false,
        }
    }

    /// Return the service name as a UTF-8 string slice.
    pub fn name_str(&self) -> &str
    {
        core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("???")
    }
}

// ── Cap classification ─────────────────────────────────────────────────────

/// Well-known capability slots discovered from the startup info.
pub struct SvcmgrCaps
{
    /// Log endpoint capability slot.
    pub log_ep: u32,
    /// Service protocol endpoint capability slot.
    pub service_ep: u32,
    /// Process manager endpoint capability slot.
    pub procmgr_ep: u32,
    /// Own address space capability slot.
    pub self_aspace: u32,
}

/// Classify startup capabilities by matching sentinel values in `aux0`.
pub fn classify_caps(startup: &StartupInfo) -> SvcmgrCaps
{
    let mut caps = SvcmgrCaps {
        log_ep: 0,
        service_ep: 0,
        procmgr_ep: 0,
        self_aspace: startup.self_aspace,
    };

    for d in startup.initial_caps
    {
        if d.cap_type == CapType::Frame
        {
            if d.aux0 == LOG_ENDPOINT_SENTINEL
            {
                caps.log_ep = d.slot;
            }
            else if d.aux0 == SERVICE_ENDPOINT_SENTINEL
            {
                caps.service_ep = d.slot;
            }
            else if d.aux0 == PROCMGR_ENDPOINT_SENTINEL
            {
                caps.procmgr_ep = d.slot;
            }
        }
    }

    caps
}
