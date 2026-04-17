// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// svcmgr/src/service.rs

//! Service table and bootstrap cap acquisition for svcmgr.
//!
//! Defines the `ServiceEntry` struct used to track monitored services and the
//! `SvcmgrCaps` struct for well-known capabilities acquired via the bootstrap
//! protocol at startup.

use ipc::IpcBuf;
use process_abi::StartupInfo;

/// Maximum number of monitored services.
pub const MAX_SERVICES: usize = 16;

/// Maximum number of extra named caps stored per service for restart.
///
/// Constrained by the 4-cap IPC message limit: a single `REGISTER_SERVICE`
/// delivers `thread + module + log + 1 extra`. Larger bundles would need
/// multi-round registration; defer until a concrete consumer appears.
pub const MAX_BUNDLE_CAPS: usize = 1;

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
    /// Extra named restart-bundle caps beyond `thread/module/log`. Each
    /// entry is re-derived and re-delivered over the bootstrap protocol
    /// after a restart so the child comes back with its full cap set.
    pub bundle: [registry::Entry; MAX_BUNDLE_CAPS],
    /// Number of valid entries at the front of `bundle`.
    pub bundle_count: u8,
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
    /// Per-child token used on the svcmgr bootstrap endpoint for restart
    /// bootstrap (`cap_derive_token(svcmgr_bootstrap_ep, SEND, token)`).
    pub bootstrap_token: u64,
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
            bundle: [registry::Entry {
                name: [0; registry::NAME_MAX],
                name_len: 0,
                cap: 0,
            }; MAX_BUNDLE_CAPS],
            bundle_count: 0,
            restart_policy: 0,
            criticality: 0,
            event_queue_cap: 0,
            restart_count: 0,
            active: false,
            bootstrap_token: 0,
        }
    }

    /// Return the service name as a UTF-8 string slice.
    pub fn name_str(&self) -> &str
    {
        core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("???")
    }
}

// ── Bootstrap ───────────────────────────────────────────────────────────────
//
// init → svcmgr bootstrap plan (one round, 4 caps):
//   caps[0]: log endpoint
//   caps[1]: service endpoint (svcmgr receives on this for registrations)
//   caps[2]: procmgr service endpoint (svcmgr uses this for restarts)
//   caps[3]: svcmgr's own bootstrap endpoint (svcmgr receives on this when
//            serving bootstrap requests from restarted children)

/// Well-known capability slots acquired from the bootstrap protocol.
#[allow(clippy::struct_field_names)]
pub struct SvcmgrCaps
{
    /// Log endpoint capability slot.
    pub log_ep: u32,
    /// Service protocol endpoint capability slot.
    pub service_ep: u32,
    /// Process manager service endpoint capability slot.
    pub procmgr_ep: u32,
    /// svcmgr's own bootstrap endpoint (receives bootstrap requests from
    /// restarted children).
    pub bootstrap_ep: u32,
}

/// Acquire svcmgr's initial cap set from its creator (init) via bootstrap IPC.
pub fn bootstrap_caps(startup: &StartupInfo, ipc: IpcBuf) -> Option<SvcmgrCaps>
{
    if startup.creator_endpoint == 0
    {
        return None;
    }
    let round = ipc::bootstrap::request_round(startup.creator_endpoint, ipc).ok()?;
    if round.cap_count < 4 || !round.done
    {
        return None;
    }
    Some(SvcmgrCaps {
        log_ep: round.caps[0],
        service_ep: round.caps[1],
        procmgr_ep: round.caps[2],
        bootstrap_ep: round.caps[3],
    })
}
