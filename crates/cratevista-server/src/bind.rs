//! Bind-first port policy.
//!
//! The listener is always **bound before** the URL is reported, and the real
//! address comes from [`tokio::net::TcpListener::local_addr`] — never from a
//! pre-bind probe (which would race). The default port `7420` increments through
//! `7421..=7440` on conflict; an **explicit** port fails immediately if occupied.

use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::error::ServerError;
use crate::options::{BindOptions, DEFAULT_PORT, PORT_RANGE_END};

/// Binds a loopback (by default) TCP listener per the port policy.
///
/// - Explicit port: bind exactly that port; any failure (including `AddrInUse`)
///   is fatal.
/// - Default/implicit port: try `port` (or `7420`) and increment to
///   `7440`, skipping only `AddrInUse`; any other error is fatal; exhausting the
///   range yields [`ServerError::PortRangeExhausted`].
/// - Port `0`: the OS assigns an ephemeral port; read it from `local_addr()`.
pub async fn bind_listener(options: &BindOptions) -> Result<TcpListener, ServerError> {
    let host = options.host;
    let start = options.port.unwrap_or(DEFAULT_PORT);

    if options.port_was_explicit {
        let addr = SocketAddr::new(host, start);
        return TcpListener::bind(addr)
            .await
            .map_err(|error| ServerError::BindFailed(format!("{addr}: {}", error.kind())));
    }

    // Port 0 (implicit) → single ephemeral bind.
    if start == 0 {
        let addr = SocketAddr::new(host, 0);
        return TcpListener::bind(addr)
            .await
            .map_err(|error| ServerError::BindFailed(format!("{addr}: {}", error.kind())));
    }

    let end = start.max(PORT_RANGE_END);
    for port in start..=end {
        let addr = SocketAddr::new(host, port);
        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(error) => {
                // A non-conflict error (e.g. permission) is fatal; do not keep
                // walking ports.
                return Err(ServerError::BindFailed(format!("{addr}: {}", error.kind())));
            }
        }
    }
    Err(ServerError::PortRangeExhausted { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn loopback() -> IpAddr {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }

    #[tokio::test]
    async fn binds_loopback_ephemeral_port() {
        let options = BindOptions {
            host: loopback(),
            port: Some(0),
            port_was_explicit: true,
        };
        let listener = bind_listener(&options).await.unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        assert_ne!(addr.port(), 0, "OS assigned a real port");
    }

    #[tokio::test]
    async fn explicit_conflict_fails_immediately() {
        // Occupy an ephemeral port, then demand it explicitly.
        let first = TcpListener::bind(SocketAddr::new(loopback(), 0))
            .await
            .unwrap();
        let taken = first.local_addr().unwrap().port();
        let options = BindOptions {
            host: loopback(),
            port: Some(taken),
            port_was_explicit: true,
        };
        let error = bind_listener(&options).await.unwrap_err();
        assert_eq!(error.code(), "bind_failed");
    }

    #[tokio::test]
    async fn implicit_conflict_increments_to_next_port() {
        // Occupy a port, then ask implicitly starting at that port: it should
        // increment to the next free one rather than fail.
        let first = TcpListener::bind(SocketAddr::new(loopback(), 0))
            .await
            .unwrap();
        let taken = first.local_addr().unwrap().port();
        // Only meaningful when there is room to increment.
        if taken >= PORT_RANGE_END {
            return;
        }
        let options = BindOptions {
            host: loopback(),
            port: Some(taken),
            port_was_explicit: false,
        };
        let listener = bind_listener(&options).await.unwrap();
        assert_ne!(listener.local_addr().unwrap().port(), taken);
    }
}
