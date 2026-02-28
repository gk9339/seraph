// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/loader/src/config.rs

//! Boot configuration parser for `\EFI\seraph\boot.conf`.
//!
//! Reads and parses the Seraph boot configuration file from the ESP,
//! extracting the paths to the kernel ELF and init module. The file
//! format is a simple `key=value` text format with `#` comments.

/// Maximum number of characters in a config-file path value.
pub const MAX_PATH_LEN: usize = 256;

/// Maximum size of the boot configuration file in bytes.
pub const MAX_CONFIG_SIZE: usize = 4096;

/// A null-terminated UTF-16 path string backed by a fixed-size stack buffer.
///
/// Used to pass file paths to EFI_FILE_PROTOCOL->Open() without heap allocation.
pub struct Utf16Path
{
    buf: [u16; MAX_PATH_LEN + 1],
    len: usize,
}

impl Utf16Path
{
    /// Return a pointer to the null-terminated UTF-16 string.
    /// The returned pointer is valid as long as this struct is alive.
    pub fn as_ptr(&self) -> *const u16
    {
        self.buf.as_ptr()
    }
}

/// Paths loaded from `\EFI\seraph\boot.conf`.
pub struct BootConfig
{
    pub kernel_path: Utf16Path,
    pub init_path: Utf16Path,
}

/// Widen an ASCII byte slice into a `Utf16Path`.
///
/// Returns `false` if any byte is non-ASCII (> 0x7F) or if the path
/// exceeds `MAX_PATH_LEN` characters. On success, the buffer is
/// null-terminated and `len` is set to the number of code units written
/// (not counting the null terminator).
fn ascii_to_utf16(path: &[u8], out: &mut Utf16Path) -> bool
{
    if path.len() > MAX_PATH_LEN
    {
        return false;
    }
    for (i, &b) in path.iter().enumerate()
    {
        if b > 0x7F
        {
            return false;
        }
        out.buf[i] = b as u16;
    }
    out.buf[path.len()] = 0u16;
    out.len = path.len();
    true
}

/// Parse a `boot.conf` byte slice into a `BootConfig`.
///
/// Format: one `key=value` pair per line, `#` introduces a comment,
/// blank lines are ignored. Unknown keys are silently skipped for
/// forward compatibility. Both `kernel` and `init` keys are required;
/// a missing key returns `BootError::InvalidConfig`.
///
/// Whitespace (space, tab, `\r`) is trimmed from both key and value.
/// Value strings must be ASCII; non-ASCII values return
/// `BootError::InvalidConfig`.
pub fn parse_config(data: &[u8]) -> Result<BootConfig, crate::error::BootError>
{
    // Use MaybeUninit to build BootConfig field-by-field without a Default impl.
    let mut kernel_path: Option<Utf16Path> = None;
    let mut init_path: Option<Utf16Path> = None;

    // Split on '\n'; handle '\r\n' line endings by trimming '\r' below.
    let mut remaining = data;
    loop
    {
        // Find the next newline or consume the whole slice.
        let (line_bytes, rest) = match remaining.iter().position(|&b| b == b'\n')
        {
            Some(pos) => (&remaining[..pos], &remaining[pos + 1..]),
            None => (remaining, &[][..]),
        };
        remaining = rest;

        // Trim trailing '\r' and leading/trailing whitespace.
        let line = trim_ascii(line_bytes);

        // Skip blank lines and comment lines.
        if line.is_empty() || line[0] == b'#'
        {
            if remaining.is_empty() && line_bytes == data
            {
                break;
            }
            if rest.is_empty() && line_bytes.len() == data.len()
            {
                break;
            }
            // (continue to next iteration)
        }
        else
        {
            // Find '=' separator.
            match line.iter().position(|&b| b == b'=')
            {
                None =>
                {
                    return Err(crate::error::BootError::InvalidConfig(
                        "missing '=' in line",
                    ))
                }
                Some(eq) =>
                {
                    let key = trim_ascii(&line[..eq]);
                    let value = trim_ascii(&line[eq + 1..]);

                    if key == b"kernel"
                    {
                        let mut p = Utf16Path {
                            buf: [0u16; MAX_PATH_LEN + 1],
                            len: 0,
                        };
                        if !ascii_to_utf16(value, &mut p)
                        {
                            return Err(crate::error::BootError::InvalidConfig(
                                "kernel path invalid",
                            ));
                        }
                        kernel_path = Some(p);
                    }
                    else if key == b"init"
                    {
                        let mut p = Utf16Path {
                            buf: [0u16; MAX_PATH_LEN + 1],
                            len: 0,
                        };
                        if !ascii_to_utf16(value, &mut p)
                        {
                            return Err(crate::error::BootError::InvalidConfig(
                                "init path invalid",
                            ));
                        }
                        init_path = Some(p);
                    }
                    // Unknown keys: skip silently (forward compatibility).
                }
            }
        }

        if remaining.is_empty()
        {
            break;
        }
    }

    Ok(BootConfig {
        kernel_path: kernel_path.ok_or(crate::error::BootError::InvalidConfig(
            "missing 'kernel' key",
        ))?,
        init_path: init_path.ok_or(crate::error::BootError::InvalidConfig("missing 'init' key"))?,
    })
}

/// Trim leading and trailing ASCII whitespace (space, tab, `\r`) from a byte slice.
fn trim_ascii(s: &[u8]) -> &[u8]
{
    let is_ws = |b: &u8| *b == b' ' || *b == b'\t' || *b == b'\r';
    let start = s.iter().position(|b| !is_ws(b)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|b| !is_ws(b))
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end
    {
        &[]
    }
    else
    {
        &s[start..end]
    }
}

/// Open `\EFI\seraph\boot.conf`, read it into a 4096-byte stack buffer, and
/// parse it with [`parse_config`].
///
/// # Safety
/// `esp_root` must be a valid `EFI_FILE_PROTOCOL` directory handle.
pub unsafe fn load_boot_config(
    esp_root: *mut crate::uefi::EfiFileProtocol,
) -> Result<BootConfig, crate::error::BootError>
{
    // Null-terminated UTF-16 path to the config file.
    // '\', 'E', 'F', 'I', '\', 's', 'e', 'r', 'a', 'p', 'h', '\', 'b', 'o', 'o', 't', '.', 'c', 'o', 'n', 'f', NUL
    static BOOT_CONF_PATH: [u16; 22] = [
        b'\\' as u16,
        b'E' as u16,
        b'F' as u16,
        b'I' as u16,
        b'\\' as u16,
        b's' as u16,
        b'e' as u16,
        b'r' as u16,
        b'a' as u16,
        b'p' as u16,
        b'h' as u16,
        b'\\' as u16,
        b'b' as u16,
        b'o' as u16,
        b'o' as u16,
        b't' as u16,
        b'.' as u16,
        b'c' as u16,
        b'o' as u16,
        b'n' as u16,
        b'f' as u16,
        0u16,
    ];

    // SAFETY: esp_root is a valid directory handle; path is null-terminated UTF-16.
    let conf_file = unsafe {
        crate::uefi::open_file(
            esp_root,
            BOOT_CONF_PATH.as_ptr(),
            "\\EFI\\seraph\\boot.conf",
        )?
    };
    // SAFETY: conf_file is a valid open file handle.
    let conf_size = unsafe { crate::uefi::file_size(conf_file)? } as usize;
    if conf_size > MAX_CONFIG_SIZE
    {
        return Err(crate::error::BootError::InvalidConfig(
            "boot.conf exceeds maximum size",
        ));
    }

    // Read into a stack buffer — no heap allocation needed.
    let mut buf = [0u8; MAX_CONFIG_SIZE];
    // SAFETY: conf_file is open at position 0; buf[..conf_size] is within the allocation.
    unsafe { crate::uefi::file_read(conf_file, &mut buf[..conf_size])? };

    parse_config(&buf[..conf_size])
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn parse_config_succeeds_with_both_keys()
    {
        let input = b"kernel=\\EFI\\seraph\\seraph-kernel\ninit=\\sbin\\init\n";
        let cfg = parse_config(input).unwrap();
        assert_eq!(cfg.kernel_path.len, 31);
        assert_eq!(cfg.init_path.len, 10);
    }

    #[test]
    fn parse_config_skips_comments_and_blank_lines()
    {
        let input = b"# comment\n\nkernel=\\EFI\\seraph\\seraph-kernel\ninit=\\sbin\\init\n";
        let cfg = parse_config(input).unwrap();
        assert_eq!(cfg.kernel_path.len, 31);
    }

    #[test]
    fn parse_config_trims_whitespace()
    {
        let input = b"kernel = \\EFI\\seraph\\seraph-kernel\r\ninit = \\sbin\\init\r\n";
        let cfg = parse_config(input).unwrap();
        assert_eq!(cfg.kernel_path.len, 31);
    }

    #[test]
    fn parse_config_missing_kernel_returns_error()
    {
        let input = b"init=\\sbin\\init\n";
        assert!(parse_config(input).is_err());
    }

    #[test]
    fn parse_config_missing_init_returns_error()
    {
        let input = b"kernel=\\EFI\\seraph\\seraph-kernel\n";
        assert!(parse_config(input).is_err());
    }

    #[test]
    fn parse_config_missing_equals_returns_error()
    {
        let input = b"kernel\\EFI\\seraph\\seraph-kernel\ninit=\\sbin\\init\n";
        assert!(parse_config(input).is_err());
    }

    #[test]
    fn parse_config_unknown_keys_are_skipped()
    {
        let input = b"kernel=\\EFI\\seraph\\seraph-kernel\ninit=\\sbin\\init\nfoo=bar\n";
        assert!(parse_config(input).is_ok());
    }

    #[test]
    fn trim_ascii_removes_leading_and_trailing_whitespace()
    {
        let result = trim_ascii(b"  hello  ");
        assert_eq!(result, b"hello");
    }

    #[test]
    fn trim_ascii_returns_empty_slice_for_all_whitespace()
    {
        let result = trim_ascii(b"   \t\r  ");
        assert_eq!(result, b"");
    }

    #[test]
    fn ascii_to_utf16_widens_correctly()
    {
        let mut p = Utf16Path {
            buf: [0u16; MAX_PATH_LEN + 1],
            len: 0,
        };
        let ok = ascii_to_utf16(b"\\sbin\\init", &mut p);
        assert!(ok);
        assert_eq!(p.len, 10);
        assert_eq!(p.buf[0], b'\\' as u16);
        assert_eq!(p.buf[10], 0u16);
    }

    #[test]
    fn ascii_to_utf16_rejects_non_ascii()
    {
        let mut p = Utf16Path {
            buf: [0u16; MAX_PATH_LEN + 1],
            len: 0,
        };
        let ok = ascii_to_utf16(b"path\x80here", &mut p);
        assert!(!ok);
    }
}
