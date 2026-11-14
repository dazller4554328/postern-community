//! Offline IP → country lookup for the forensics delivery-path view.
//!
//! Reads a MaxMind-format MMDB file (GeoLite2-Country, dbip-country-lite,
//! or any other country-flavoured MMDB). The reader is loaded once on
//! first use and cached — failure to load (file missing, corrupt) is
//! sticky, so we don't retry on every hop.
//!
//! Resolution order for the database path:
//!   1. `POSTERN_GEOIP_DB` env var
//!   2. `<POSTERN_DATA_DIR>/GeoLite2-Country.mmdb`
//!   3. `<POSTERN_DATA_DIR>/dbip-country-lite.mmdb`
//!   4. `/usr/share/GeoIP/GeoLite2-Country.mmdb`
//!
//! No file → all lookups return `None`. The forensics tab still works,
//! it just doesn't show countries.

use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use maxminddb::{geoip2, Reader};

static READER: OnceLock<Option<Reader<Vec<u8>>>> = OnceLock::new();

fn candidate_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("POSTERN_GEOIP_DB") {
        out.push(PathBuf::from(p));
    }
    let data_dir =
        std::env::var("POSTERN_DATA_DIR").map_or_else(|_| PathBuf::from("./data"), PathBuf::from);
    out.push(data_dir.join("GeoLite2-Country.mmdb"));
    out.push(data_dir.join("dbip-country-lite.mmdb"));
    out.push(PathBuf::from("/usr/share/GeoIP/GeoLite2-Country.mmdb"));
    out
}

fn load() -> Option<Reader<Vec<u8>>> {
    for path in candidate_paths() {
        if !Path::new(&path).exists() {
            continue;
        }
        match Reader::open_readfile(&path) {
            Ok(r) => {
                tracing::info!(db = %path.display(), "geoip database loaded");
                return Some(r);
            }
            Err(e) => {
                tracing::warn!(db = %path.display(), error = %e, "geoip database failed to load");
            }
        }
    }
    tracing::info!("no geoip database found — forensics country lookup disabled");
    None
}

fn reader() -> Option<&'static Reader<Vec<u8>>> {
    READER.get_or_init(load).as_ref()
}

/// Look up the ISO country code (e.g. `US`) and human-readable name
/// (e.g. `United States`) for an IP. Returns `None` if no database is
/// installed, the IP is private/unrouteable, or the IP isn't in the DB.
pub fn lookup(ip: IpAddr) -> Option<(String, String)> {
    if !is_public(ip) {
        return None;
    }
    let r = reader()?;
    let country: geoip2::Country = r.lookup(ip).ok()?;
    let c = country.country?;
    let code = c.iso_code?.to_owned();
    let name = c
        .names
        .as_ref()
        .and_then(|m| m.get("en").copied())
        .map_or_else(|| code.clone(), std::borrow::ToOwned::to_owned);
    Some((code, name))
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return false;
            }
            let s = v6.segments();
            // fc00::/7 ULA, fe80::/10 link-local, 2001:db8::/32 doc,
            // 2002::/16 6to4 (also used as Google internal server IDs).
            !((s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || s[0] == 0x2002)
        }
    }
}
