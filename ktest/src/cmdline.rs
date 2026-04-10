// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/cmdline.rs

//! Kernel command line parser for ktest-specific options.
//!
//! Parses space-separated `key=value` tokens from the raw command line bytes
//! received via the init protocol. Only tokens prefixed with `ktest.` are
//! consumed; all others are silently ignored.
//!
//! Supported options:
//! - `ktest.shutdown=always|pass|never` — when to shut down after tests
//! - `ktest.timeout=N` — seconds to wait before shutdown (decimal integer)

/// When to perform system shutdown after tests complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownPolicy
{
    /// Shut down regardless of test outcome.
    Always,
    /// Shut down only if all tests passed.
    Pass,
    /// Never shut down (current default; halt in place).
    Never,
}

/// Parsed ktest configuration from the kernel command line.
pub struct KtestConfig
{
    pub shutdown_policy: ShutdownPolicy,
    pub timeout_secs: u32,
}

impl KtestConfig
{
    const DEFAULT: KtestConfig = KtestConfig {
        shutdown_policy: ShutdownPolicy::Never,
        timeout_secs: 0,
    };
}

/// Parse ktest options from raw command line bytes.
///
/// Tokens are space-separated. Unknown keys are ignored.
pub fn parse(cmdline: &[u8]) -> KtestConfig
{
    let mut config = KtestConfig::DEFAULT;

    for token in CmdlineTokens::new(cmdline)
    {
        if let Some(rest) = strip_prefix(token, b"ktest.shutdown=")
        {
            config.shutdown_policy = match rest
            {
                b"always" => ShutdownPolicy::Always,
                b"pass" => ShutdownPolicy::Pass,
                b"never" => ShutdownPolicy::Never,
                _ => config.shutdown_policy,
            };
        }
        else if let Some(rest) = strip_prefix(token, b"ktest.timeout=")
        {
            config.timeout_secs = parse_u32(rest).unwrap_or(config.timeout_secs);
        }
    }

    config
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn strip_prefix<'a>(s: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]>
{
    if s.len() >= prefix.len() && &s[..prefix.len()] == prefix
    {
        Some(&s[prefix.len()..])
    }
    else
    {
        None
    }
}

fn parse_u32(s: &[u8]) -> Option<u32>
{
    if s.is_empty()
    {
        return None;
    }
    let mut val: u32 = 0;
    for &b in s
    {
        if !b.is_ascii_digit()
        {
            return None;
        }
        val = val.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(val)
}

/// Iterator over space-separated tokens in a byte slice.
struct CmdlineTokens<'a>
{
    data: &'a [u8],
    pos: usize,
}

impl<'a> CmdlineTokens<'a>
{
    fn new(data: &'a [u8]) -> Self
    {
        Self { data, pos: 0 }
    }
}

impl<'a> Iterator for CmdlineTokens<'a>
{
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]>
    {
        // Skip leading spaces.
        while self.pos < self.data.len() && self.data[self.pos] == b' '
        {
            self.pos += 1;
        }
        if self.pos >= self.data.len()
        {
            return None;
        }
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != b' '
        {
            self.pos += 1;
        }
        Some(&self.data[start..self.pos])
    }
}
