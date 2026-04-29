//! VPN — community build.
//!
//! Postern Community doesn't ship a VPN tunnel. The pro build
//! (https://github.com/dazller4554328/postern) wires WireGuard with
//! a kill-switch, NordLynx auto-config, and egress binding via
//! SO_BINDTODEVICE; that code is VPS-centric and makes no sense on a
//! personal machine where the user already controls their own VPN.
//!
//! This stub exposes the same `VpnManager` + `VpnStatus` surface the
//! rest of the codebase expects, but every operation is a no-op and
//! status is permanently "off". Every existing call site
//! (`.status().interface_up` in the scheduler, send path, pgp
//! discover, vault reconcile) naturally takes the "no tunnel" branch.

use std::sync::Arc;

use serde::Serialize;

use crate::{privacy::ImageProxy, storage::Db};

#[derive(Debug, Clone, Default, Serialize)]
pub struct VpnStatus {
    pub enabled: bool,
    pub provider: Option<String>,
    pub region_label: Option<String>,
    pub interface_up: bool,
    pub exit_ip: Option<String>,
    pub last_check_utc: Option<i64>,
    pub last_error: Option<String>,
    pub killswitch_enabled: bool,
    pub can_refresh: bool,
    pub country_id: Option<u32>,
    pub server_load: Option<u32>,
    pub server_country_code: Option<String>,
    pub server_number: Option<u32>,
    pub server_city: Option<String>,
}

#[derive(Clone)]
pub struct VpnManager {
    _db: Arc<Db>,
    _proxy: ImageProxy,
}

impl VpnManager {
    pub fn new(db: Arc<Db>, proxy: ImageProxy) -> Self {
        tracing::info!("vpn disabled in this build (Postern Community)");
        Self { _db: db, _proxy: proxy }
    }

    pub fn status(&self) -> VpnStatus {
        VpnStatus::default()
    }

    pub fn reconcile_on_boot(&self) -> crate::error::Result<()> {
        Ok(())
    }

    /// Network policy for outbound clients. There is no kill-switch
    /// in the community build — outbound traffic uses the host's
    /// default route — so callers should bind to nothing in
    /// particular.
    pub fn bind_iface(&self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NordMeta {
    pub country_id: Option<u32>,
    pub server_load: Option<u32>,
}
