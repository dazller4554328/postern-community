//! Shared network helpers — SO_BINDTODEVICE-capable socket open and
//! a small percent-encoder used by HTTP/redirect callers.

use std::{
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use socket2::{Domain, Socket, Type};

use crate::error::{Error, Result};

/// RFC-3986 percent-encode a string. Letters, digits, and the four
/// "unreserved" characters (`-`, `_`, `.`, `~`) pass through; every
/// other byte becomes `%XX`. Suitable for URL path segments and
/// query values; over-encodes a few characters that would also be
/// legal in query values (e.g. `@`), but the over-encoding is
/// harmless for our HTML/JS consumers.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        let c = *byte;
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_unreserved() {
        assert_eq!(urlencode("abcXYZ-_.~"), "abcXYZ-_.~");
    }

    #[test]
    fn encodes_reserved_and_high_bytes() {
        assert_eq!(urlencode("a@b"), "a%40b");
        assert_eq!(urlencode(" "), "%20");
        assert_eq!(urlencode("/?#"), "%2F%3F%23");
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Open a TCP connection to `host:port`. When `bind_iface` is set, the
/// socket is pinned to that network interface via SO_BINDTODEVICE —
/// this is how we route through `wg0` when the VPN kill-switch is active.
pub fn open_tcp(host: &str, port: u16, bind_iface: Option<&str>) -> Result<TcpStream> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| Error::Other(anyhow::anyhow!("resolve {host}: {e}")))?
        .next()
        .ok_or_else(|| Error::Other(anyhow::anyhow!("no A records for {host}")))?;
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let socket = Socket::new(domain, Type::STREAM, None)
        .map_err(|e| Error::Other(anyhow::anyhow!("socket: {e}")))?;
    if let Some(name) = bind_iface {
        socket
            .bind_device(Some(name.as_bytes()))
            .map_err(|e| Error::Other(anyhow::anyhow!("bind_device {name}: {e}")))?;
    }
    socket
        .connect_timeout(&addr.into(), CONNECT_TIMEOUT)
        .map_err(|e| Error::Other(anyhow::anyhow!("connect {addr}: {e}")))?;
    Ok(socket.into())
}
