//! Configuration value types for snapshot loading, binding, and source access.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;

/// The three artifact file paths a snapshot is loaded from.
#[derive(Debug, Clone)]
pub struct ArtifactPaths {
    /// `<out>/document.json`.
    pub document: PathBuf,
    /// `<out>/generation.json` (the completion marker).
    pub generation: PathBuf,
    /// `<out>/diagnostics.json`.
    pub diagnostics: PathBuf,
}

impl ArtifactPaths {
    /// Builds the standard triple under an output directory (usually
    /// `target/cratevista`).
    pub fn in_dir(output_dir: &std::path::Path) -> Self {
        ArtifactPaths {
            document: output_dir.join("document.json"),
            generation: output_dir.join("generation.json"),
            diagnostics: output_dir.join("diagnostics.json"),
        }
    }
}

/// Bounded-retry and version-gating policy for [`crate::load_snapshot`].
#[derive(Debug, Clone)]
pub struct SnapshotLoadOptions {
    /// Maximum number of extra attempts after the first (default 4).
    pub max_retries: u32,
    /// Delay between attempts (default ~25 ms).
    pub retry_delay: Duration,
    /// The supported `SchemaVersion` major (default 1).
    pub supported_major: u32,
}

impl Default for SnapshotLoadOptions {
    fn default() -> Self {
        SnapshotLoadOptions {
            max_retries: 4,
            retry_delay: Duration::from_millis(25),
            supported_major: 1,
        }
    }
}

/// The default loopback host.
pub const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// The default port.
pub const DEFAULT_PORT: u16 = 7420;
/// The last port tried when incrementing from the default on conflict.
pub const PORT_RANGE_END: u16 = 7440;

/// Bind configuration. This is the single option struct for host/port; there is
/// no separate `ServerOptions` duplicating these fields.
#[derive(Debug, Clone)]
pub struct BindOptions {
    /// The host to bind (default `127.0.0.1`).
    pub host: IpAddr,
    /// The requested port, or `None` for the default with increment-on-conflict.
    pub port: Option<u16>,
    /// Whether the port was set explicitly (explicit conflicts fail immediately).
    pub port_was_explicit: bool,
}

impl Default for BindOptions {
    fn default() -> Self {
        BindOptions {
            host: DEFAULT_HOST,
            port: None,
            port_was_explicit: false,
        }
    }
}

impl BindOptions {
    /// Whether the bind host is a loopback address.
    pub fn is_loopback(&self) -> bool {
        self.host.is_loopback()
    }
}

/// Whether the guarded `/api/source` endpoint serves file contents.
#[derive(Debug, Clone, Default)]
pub enum SourceAccessPolicy {
    /// Source contents are never served (`403`). The default.
    #[default]
    Disabled,
    /// Source contents under `root` (already canonicalized) may be served, up to
    /// `max_bytes`.
    Enabled {
        /// The canonicalized project root that all requests must resolve under.
        root: PathBuf,
        /// The maximum file size served, in bytes.
        max_bytes: u64,
    },
}
