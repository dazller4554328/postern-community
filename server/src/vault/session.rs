//! Per-device session state machine.
//!
//! Lifted out of `vault/mod.rs` so the session-lifetime rules — idle
//! window, hard cap, IP-pin with tailnet exemption — sit in one
//! auditable file with their tests. The vault still owns the
//! `HashMap<token_hash, DeviceSession>` and decides what to do with
//! the `SessionVerdict` (audit log, evict, refresh `last_seen_at`);
//! this module is the pure decision function.

/// Sliding-window idle limit. Any request bearing this session's
/// cookie that arrives more than `SESSION_IDLE_SECS` after the last
/// request expires the session. 30 minutes covers a laptop that
/// suspends mid-task without re-prompting users on a working machine.
pub const SESSION_IDLE_SECS: i64 = 30 * 60;

/// Hard ceiling on session lifetime, no extensions. Forces password
/// re-entry once a working day even when the tab has been polling
/// non-stop (desktop with screen lock + always-on browser). Tuned to
/// 8 hours so it lines up with one work session.
pub const SESSION_HARD_CAP_SECS: i64 = 8 * 3600;

/// A single device's authenticated state. Created on a successful
/// unlock from that device, dropped on explicit lock, idle expiry,
/// hard-cap expiry, IP change, or process restart.
#[derive(Clone, Debug)]
pub(super) struct DeviceSession {
    /// IP at the moment of unlock for this device. Empty string when
    /// the request had no readable client IP header — the IP-pin
    /// behaviour matches the global path: tailnet exempt, non-tailnet
    /// pinned to the unlock value.
    pub(super) unlock_ip: String,
    /// Unix timestamp of the unlock request. Combined with
    /// `SESSION_HARD_CAP_SECS` to force re-unlock once a day even if
    /// the tab has been polling continuously.
    pub(super) opened_at: i64,
    /// Unix timestamp of the most recent request bearing this token.
    /// Combined with `SESSION_IDLE_SECS` so a laptop suspended after
    /// the user walks away gets re-locked on resume rather than
    /// auto-resuming an authenticated session.
    pub(super) last_seen_at: i64,
}

/// Verdict from evaluating a session's timestamps + IP against a
/// request. Pure function output — `session_check` translates this
/// into `Result<()>` plus side effects (audit log, eviction).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SessionVerdict {
    Ok,
    IdleExpired,
    HardCapExpired,
    IpMismatch,
}

/// Apply the session lifetime rules without touching any state.
/// Order of checks matters: idle takes priority over hard-cap (a
/// laptop suspended overnight should report "idle" not "expired"),
/// and IP mismatch is checked last so a long-suspended session on a
/// new network reports the timing reason rather than the IP one.
pub(super) fn evaluate_session(
    now: i64,
    session: &DeviceSession,
    current_ip: &str,
) -> SessionVerdict {
    if now - session.last_seen_at > SESSION_IDLE_SECS {
        return SessionVerdict::IdleExpired;
    }
    if now - session.opened_at > SESSION_HARD_CAP_SECS {
        return SessionVerdict::HardCapExpired;
    }
    if !is_tailnet_cgnat(current_ip)
        && !is_tailnet_cgnat(&session.unlock_ip)
        && !session.unlock_ip.is_empty()
        && session.unlock_ip != current_ip
    {
        return SessionVerdict::IpMismatch;
    }
    SessionVerdict::Ok
}

/// True when `ip` falls inside the RFC 6598 CGNAT block `100.64.0.0/10`,
/// which is what Tailscale hands out for every tailnet device. We parse
/// leniently: IPv6 addresses, `unknown`, and malformed strings all
/// return false so the normal pin logic applies.
pub(super) fn is_tailnet_cgnat(ip: &str) -> bool {
    let trimmed = ip.trim();
    // Strip a possible `[host]:port` or `host:port` suffix; we only
    // care about the address part.
    let addr = trimmed.split(',').next().unwrap_or(trimmed).trim();
    let octets: Vec<&str> = addr.split('.').collect();
    if octets.len() != 4 {
        return false;
    }
    let first: u8 = match octets[0].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let second: u8 = match octets[1].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    // 100.64.0.0/10 → first octet 100, top two bits of second octet = 01
    first == 100 && (64..=127).contains(&second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(opened_at: i64, last_seen_at: i64, unlock_ip: &str) -> DeviceSession {
        DeviceSession {
            opened_at,
            last_seen_at,
            unlock_ip: unlock_ip.to_string(),
        }
    }

    #[test]
    fn fresh_session_is_ok() {
        let now = 1_000_000;
        let s = session(now, now, "100.96.19.65");
        assert_eq!(evaluate_session(now, &s, "100.96.19.65"), SessionVerdict::Ok);
    }

    #[test]
    fn idle_window_just_inside_passes() {
        let now = 1_000_000;
        let s = session(now - 60, now - SESSION_IDLE_SECS, "100.96.19.65");
        assert_eq!(evaluate_session(now, &s, "100.96.19.65"), SessionVerdict::Ok);
    }

    #[test]
    fn idle_window_one_past_expires() {
        let now = 1_000_000;
        let s = session(now - 60, now - SESSION_IDLE_SECS - 1, "100.96.19.65");
        assert_eq!(
            evaluate_session(now, &s, "100.96.19.65"),
            SessionVerdict::IdleExpired
        );
    }

    #[test]
    fn hard_cap_one_past_expires() {
        // last_seen recent (so idle is fine), opened long ago.
        let now = 1_000_000;
        let s = session(now - SESSION_HARD_CAP_SECS - 1, now - 5, "100.96.19.65");
        assert_eq!(
            evaluate_session(now, &s, "100.96.19.65"),
            SessionVerdict::HardCapExpired
        );
    }

    #[test]
    fn idle_takes_priority_over_hard_cap() {
        // Both expired — idle should win so the user gets the more
        // specific message ("you walked away") rather than the
        // catch-all ("session expired").
        let now = 1_000_000;
        let s = session(
            now - SESSION_HARD_CAP_SECS - 1,
            now - SESSION_IDLE_SECS - 1,
            "100.96.19.65",
        );
        assert_eq!(
            evaluate_session(now, &s, "100.96.19.65"),
            SessionVerdict::IdleExpired
        );
    }

    #[test]
    fn tailnet_request_passes_regardless_of_unlock_ip() {
        let now = 1_000_000;
        // Pinned to a public IP, but the request comes from tailnet —
        // CGNAT exemption applies.
        let s = session(now, now, "1.2.3.4");
        assert_eq!(
            evaluate_session(now, &s, "100.96.19.65"),
            SessionVerdict::Ok
        );
    }

    #[test]
    fn tailnet_unlock_ip_passes_for_any_request() {
        let now = 1_000_000;
        let s = session(now, now, "100.96.19.65");
        // Unlocked from tailnet → never pin for any source IP. This
        // is the household case (laptop on tailnet originally,
        // request comes through tunnel).
        assert_eq!(evaluate_session(now, &s, "1.2.3.4"), SessionVerdict::Ok);
    }

    #[test]
    fn empty_unlock_ip_passes() {
        let now = 1_000_000;
        // Unlock with no readable client IP (e.g., direct loopback)
        // shouldn't pin to "" and reject every later request.
        let s = session(now, now, "");
        assert_eq!(evaluate_session(now, &s, "1.2.3.4"), SessionVerdict::Ok);
    }

    #[test]
    fn public_ip_mismatch_is_rejected() {
        let now = 1_000_000;
        let s = session(now, now, "1.2.3.4");
        assert_eq!(
            evaluate_session(now, &s, "5.6.7.8"),
            SessionVerdict::IpMismatch
        );
    }

    #[test]
    fn same_public_ip_is_ok() {
        let now = 1_000_000;
        let s = session(now, now, "1.2.3.4");
        assert_eq!(evaluate_session(now, &s, "1.2.3.4"), SessionVerdict::Ok);
    }

    #[test]
    fn accepts_cgnat_range() {
        assert!(is_tailnet_cgnat("100.64.0.0"));
        assert!(is_tailnet_cgnat("100.96.19.65"));
        assert!(is_tailnet_cgnat("100.101.73.19"));
        assert!(is_tailnet_cgnat("100.127.255.255"));
    }

    #[test]
    fn rejects_outside_cgnat() {
        // Just below the range.
        assert!(!is_tailnet_cgnat("100.63.255.255"));
        // Just above.
        assert!(!is_tailnet_cgnat("100.128.0.0"));
        // Other 100.0.0.0/8 but outside /10.
        assert!(!is_tailnet_cgnat("100.0.0.1"));
        // Random public IPs.
        assert!(!is_tailnet_cgnat("1.2.3.4"));
        assert!(!is_tailnet_cgnat("51.178.16.33"));
        // Loopback / RFC1918.
        assert!(!is_tailnet_cgnat("127.0.0.1"));
        assert!(!is_tailnet_cgnat("192.168.1.1"));
        assert!(!is_tailnet_cgnat("10.0.0.1"));
    }

    #[test]
    fn rejects_malformed_or_ipv6() {
        assert!(!is_tailnet_cgnat(""));
        assert!(!is_tailnet_cgnat("unknown"));
        assert!(!is_tailnet_cgnat("100.64.0"));
        assert!(!is_tailnet_cgnat("100.abc.0.0"));
        assert!(!is_tailnet_cgnat("fd7a:115c:a1e0::1"));
    }

    #[test]
    fn tolerates_xff_list_and_whitespace() {
        // X-Forwarded-For can contain a comma-separated chain.
        assert!(is_tailnet_cgnat("100.96.19.65, 10.0.0.1"));
        assert!(is_tailnet_cgnat("  100.96.19.65  "));
    }
}
