//! Minimal SMTP-submission client with SO_BINDTODEVICE support.
//!
//! Lettre 0.11's transport doesn't expose device-binding, so when the
//! VPN kill-switch is active (all non-`wg0` egress dropped) SMTP sends
//! fail with ENETUNREACH. This module handles STARTTLS (587) and
//! implicit TLS (465 and similar), authenticates via AUTH PLAIN, and
//! delivers a pre-built MIME blob — the bits we actually need from
//! lettre (message builder, AUTH) without giving up the bound socket.
//!
//! Not a full RFC 5321 implementation — just what submission needs.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use native_tls::{TlsConnector, TlsStream};

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Submit one message. `from` is the envelope sender (RFC 5321 MAIL FROM);
/// `rcpts` is the full recipient set including Bcc (they're already
/// stripped from the DATA blob by lettre's Message builder).
pub fn send(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    bind_iface: Option<&str>,
    from: &str,
    rcpts: &[String],
    data: &[u8],
) -> Result<()> {
    let ehlo_name = "localhost";

    if port == 465 {
        // Implicit TLS — wrap the socket before we speak a byte.
        let tcp = open_tcp_smtp(host, port, bind_iface)?;
        let connector = TlsConnector::builder()
            .build()
            .map_err(|e| Error::Other(anyhow::anyhow!("tls init: {e}")))?;
        let tls = connector
            .connect(host, tcp)
            .map_err(|e| Error::Other(anyhow::anyhow!("tls handshake: {e}")))?;
        speak_smtp(
            Stream::Tls(tls),
            ehlo_name,
            user,
            password,
            from,
            rcpts,
            data,
            true,
        )
    } else {
        // STARTTLS: plain text, EHLO, STARTTLS, then upgrade.
        let tcp = open_tcp_smtp(host, port, bind_iface)?;
        let mut stream = Stream::Plain(tcp);

        read_line_expect(&mut stream, 220)?;
        write_line(&mut stream, &format!("EHLO {ehlo_name}"))?;
        read_multi(&mut stream, 250)?;
        write_line(&mut stream, "STARTTLS")?;
        read_line_expect(&mut stream, 220)?;

        let tcp = match stream {
            Stream::Plain(t) => t,
            _ => unreachable!(),
        };
        let connector = TlsConnector::builder()
            .build()
            .map_err(|e| Error::Other(anyhow::anyhow!("tls init: {e}")))?;
        let tls = connector
            .connect(host, tcp)
            .map_err(|e| Error::Other(anyhow::anyhow!("tls handshake: {e}")))?;
        speak_smtp(
            Stream::Tls(tls),
            ehlo_name,
            user,
            password,
            from,
            rcpts,
            data,
            false,
        )
    }
}

/// Full SMTP submission dialogue once the socket (plain or TLS) is ready.
/// `need_banner`: only true for implicit-TLS; STARTTLS paths already
/// consumed the pre-TLS 220.
#[allow(clippy::too_many_arguments)]
fn speak_smtp(
    mut s: Stream,
    ehlo_name: &str,
    user: &str,
    password: &str,
    from: &str,
    rcpts: &[String],
    data: &[u8],
    need_banner: bool,
) -> Result<()> {
    if need_banner {
        read_line_expect(&mut s, 220)?;
    }
    write_line(&mut s, &format!("EHLO {ehlo_name}"))?;
    read_multi(&mut s, 250)?;

    // AUTH PLAIN with the combined \0user\0pass token.
    let token = {
        let mut buf = Vec::with_capacity(user.len() + password.len() + 2);
        buf.push(0);
        buf.extend_from_slice(user.as_bytes());
        buf.push(0);
        buf.extend_from_slice(password.as_bytes());
        B64.encode(&buf)
    };
    write_line(&mut s, &format!("AUTH PLAIN {token}"))?;
    read_multi(&mut s, 235)?;

    write_line(&mut s, &format!("MAIL FROM:<{from}>"))?;
    read_multi(&mut s, 250)?;
    for r in rcpts {
        write_line(&mut s, &format!("RCPT TO:<{r}>"))?;
        read_multi(&mut s, 250)?;
    }

    write_line(&mut s, "DATA")?;
    read_line_expect(&mut s, 354)?;
    // Dot-stuffing per RFC 5321 §4.5.2: any line starting with '.' needs
    // the dot doubled so our body isn't prematurely terminated.
    let stuffed = dot_stuff(data);
    s.write_all(&stuffed)
        .map_err(|e| Error::Other(anyhow::anyhow!("write DATA body: {e}")))?;
    // Ensure trailing CRLF before the terminating dot.
    if !stuffed.ends_with(b"\r\n") {
        s.write_all(b"\r\n")
            .map_err(|e| Error::Other(anyhow::anyhow!("write trailing CRLF: {e}")))?;
    }
    s.write_all(b".\r\n")
        .map_err(|e| Error::Other(anyhow::anyhow!("write DATA end: {e}")))?;
    read_multi(&mut s, 250)?;

    // Best-effort QUIT — ignore errors since we're done anyway.
    let _ = write_line(&mut s, "QUIT");
    Ok(())
}

fn dot_stuff(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 100);
    let mut at_line_start = true;
    for &b in data {
        if at_line_start && b == b'.' {
            out.push(b'.');
        }
        out.push(b);
        at_line_start = b == b'\n';
    }
    out
}

enum Stream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(t) => t.read(buf),
            Stream::Tls(t) => t.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(t) => t.write(buf),
            Stream::Tls(t) => t.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(t) => t.flush(),
            Stream::Tls(t) => t.flush(),
        }
    }
}

fn open_tcp_smtp(host: &str, port: u16, bind_iface: Option<&str>) -> Result<TcpStream> {
    let tcp = crate::net::open_tcp(host, port, bind_iface)?;
    tcp.set_read_timeout(Some(TIMEOUT)).ok();
    tcp.set_write_timeout(Some(TIMEOUT)).ok();
    Ok(tcp)
}

fn write_line<W: Write>(w: &mut W, line: &str) -> Result<()> {
    w.write_all(line.as_bytes())
        .map_err(|e| Error::Other(anyhow::anyhow!("smtp write: {e}")))?;
    w.write_all(b"\r\n")
        .map_err(|e| Error::Other(anyhow::anyhow!("smtp write crlf: {e}")))?;
    Ok(())
}

/// Read a single response line and require the advertised 3-digit code
/// on it. Used for phases where the server sends exactly one line
/// (banner, STARTTLS-go-ahead, DATA-go-ahead).
fn read_line_expect<R: Read>(r: &mut R, want: u16) -> Result<()> {
    let mut reader = BufReader::new(r);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| Error::Other(anyhow::anyhow!("smtp read: {e}")))?;
    parse_code(&line, want)
}

/// Read a multi-line response (EHLO et al.) — each continuation line
/// starts with "NNN-" and the final one with "NNN ". Require the code.
fn read_multi<R: Read>(s: &mut R, want: u16) -> Result<()> {
    let mut buf = [0u8; 1];
    let mut line = Vec::<u8>::new();
    loop {
        let n = s
            .read(&mut buf)
            .map_err(|e| Error::Other(anyhow::anyhow!("smtp read: {e}")))?;
        if n == 0 {
            return Err(Error::Other(anyhow::anyhow!("smtp peer closed connection")));
        }
        line.push(buf[0]);
        if line.ends_with(b"\r\n") {
            let text = String::from_utf8_lossy(&line);
            let this = parse_code(&text, want);
            // Continuation marker is the 4th byte ('-' = more coming).
            let more = line.len() >= 4 && line[3] == b'-';
            if !more {
                return this;
            }
            this?; // any line with wrong code → immediately fail
            line.clear();
        }
    }
}

fn parse_code(line: &str, want: u16) -> Result<()> {
    if line.len() < 3 {
        return Err(Error::Other(anyhow::anyhow!(
            "smtp short response: {line:?}"
        )));
    }
    let code: u16 = line[..3]
        .parse()
        .map_err(|_| Error::Other(anyhow::anyhow!("smtp bad code: {line:?}")))?;
    if code != want {
        return Err(Error::Other(anyhow::anyhow!(
            "smtp unexpected {code} (want {want}): {}",
            line.trim_end()
        )));
    }
    Ok(())
}
