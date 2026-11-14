use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{
    error::{Error, Result},
    pgp::{
        check_keyserver, discover_key, generate_keypair, parse_public_key_info,
        parse_secret_key_info, publish_to_hagrid, unprotect_secret_key, DiscoveryResult,
        DiscoverySource, KeyRow, KeySource, KeyserverStatus, NewKey, PublishResult,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pgp/keys", get(list).post(import))
        .route("/pgp/keys/generate", post(generate))
        .route("/pgp/keys/:id", axum::routing::delete(delete_key))
        .route("/pgp/keys/:id/export", get(export))
        .route("/pgp/keys/:id/publish", post(publish))
        .route("/pgp/keys/export-all", get(export_all))
        .route("/pgp/keyserver-scan", get(keyserver_scan))
        .route("/pgp/discover", get(discover))
        .route("/pgp/can-encrypt", get(can_encrypt))
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<KeyRow>>> {
    Ok(Json(s.db.pgp_list()?))
}

#[derive(Deserialize)]
struct GenerateBody {
    user_id: String,
}

async fn generate(
    State(s): State<AppState>,
    Json(body): Json<GenerateBody>,
) -> Result<Json<KeyRow>> {
    s.vault.require_unlocked()?;
    // Key generation is CPU-bound; run it on the blocking pool.
    let generated = tokio::task::spawn_blocking(move || generate_keypair(&body.user_id))
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    let info = parse_public_key_info(&generated.armored_public)?;
    let id = s.db.pgp_upsert(
        &NewKey {
            info: &info,
            armored_public: &generated.armored_public,
            armored_secret: Some(&generated.armored_secret),
            source: KeySource::Generated,
        },
        &s.vault,
    )?;
    let row =
        s.db.pgp_list()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct ImportBody {
    armored: String,
    /// Passphrase for a protected private-key paste. Ignored for public keys
    /// and for unprotected private keys.
    #[serde(default)]
    passphrase: Option<String>,
}

async fn import(State(s): State<AppState>, Json(body): Json<ImportBody>) -> Result<Json<KeyRow>> {
    let armored = body.armored.trim();
    if armored.is_empty() {
        return Err(Error::BadRequest("armored key is required".into()));
    }

    // Decide: is this a public-only paste or does it include a secret key?
    // A pasted secret key gets its passphrase stripped so the stored copy is
    // usable by Postern's (empty-passphrase) decrypt path.
    let (info, secret_armored, source) = if armored.contains("BEGIN PGP PRIVATE KEY BLOCK") {
        let info = parse_secret_key_info(armored)?;
        let unprotected = unprotect_secret_key(armored, body.passphrase.as_deref())?;
        (info, Some(unprotected), KeySource::Imported)
    } else {
        let info = parse_public_key_info(armored)?;
        (info, None, KeySource::Imported)
    };

    // Every keyring row stores the public half separately so it can be handed
    // out without touching the secret. Derive it from the (now unprotected)
    // secret key when one was pasted.
    let armored_public = if let Some(secret) = &secret_armored {
        extract_public_from_secret(secret)?
    } else {
        armored.to_owned()
    };

    s.vault.require_unlocked()?;
    let id = s.db.pgp_upsert(
        &NewKey {
            info: &info,
            armored_public: &armored_public,
            armored_secret: secret_armored.as_deref(),
            source,
        },
        &s.vault,
    )?;
    let row =
        s.db.pgp_list()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
    Ok(Json(row))
}

/// When the user pastes a private-key block, we derive the armored public
/// half by signing the inner public key with the secret. Every keyring row
/// has an `armored_public` column regardless of source.
fn extract_public_from_secret(armored_secret: &str) -> Result<String> {
    use pgp::composed::{ArmorOptions, Deserializable, SignedPublicKey, SignedSecretKey};
    use std::io::Cursor;

    let (secret, _) = SignedSecretKey::from_armor_single(Cursor::new(armored_secret))
        .map_err(|e| Error::BadRequest(format!("bad secret key: {e}")))?;
    // Derive the public half from the already-signed secret key. This reuses
    // the existing self-signatures and keeps the encryption subkey — and,
    // crucially, needs no passphrase. (Re-signing the public key would fail
    // on a passphrase-protected secret key, surfacing as an "internal error"
    // on import, and would also drop the encryption subkey.)
    let signed_pub: SignedPublicKey = secret.into();
    signed_pub
        .to_armored_string(ArmorOptions::default())
        .map_err(|e| Error::Other(anyhow::anyhow!("armor pubkey: {e}")))
}

async fn delete_key(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.db.pgp_delete(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

async fn export(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>> {
    let armored = s.db.pgp_export_public(id)?;
    Ok(Json(serde_json::json!({ "armored": armored })))
}

#[derive(Deserialize)]
struct DiscoverQuery {
    email: String,
}

async fn discover(
    State(s): State<AppState>,
    Query(q): Query<DiscoverQuery>,
) -> Result<Json<DiscoveryResult>> {
    let iface = s.proxy.bound_interface();
    Ok(Json(discover_key(&q.email, iface.as_deref()).await?))
}

#[derive(Deserialize)]
struct ExportAllQuery {
    /// Include PRIVATE KEY BLOCK sections in the bundle. Defaults to
    /// public-only, which is safe to download anywhere. Set to true
    /// for a real backup. Requires the vault to be unlocked.
    #[serde(default)]
    include_secret: bool,
}

/// Concatenate every key in the ring into one armored download.
/// Public-only by default; pass `?include_secret=true` to produce a
/// full backup suitable for re-importing into Postern or GPG.
async fn export_all(
    State(s): State<AppState>,
    Query(q): Query<ExportAllQuery>,
) -> Result<axum::response::Response> {
    if q.include_secret {
        s.vault.require_unlocked()?;
    }
    let armored = s.db.pgp_export_all(&s.vault, q.include_secret)?;
    // Build a Content-Disposition filename from the current date
    // so repeated exports don't fight each other in the downloads
    // folder. The ".asc" extension is what GPG and every OpenPGP
    // tool expects.
    let today = chrono::Utc::now().format("%Y-%m-%d");
    let filename = if q.include_secret {
        format!("postern-pgp-backup-{today}.asc")
    } else {
        format!("postern-pgp-public-{today}.asc")
    };
    use axum::http::{header, StatusCode};
    let resp = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pgp-keys")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(armored))
        .map_err(|e| Error::Other(anyhow::anyhow!("build export response: {e}")))?;
    Ok(resp)
}

/// Check whether each of the user's configured mail account
/// addresses has a published key on keys.openpgp.org. Powers the
/// Settings → PGP "Scan keyserver" button.
async fn keyserver_scan(State(s): State<AppState>) -> Result<Json<Vec<KeyserverStatus>>> {
    let addrs: Vec<String> = s.db.list_accounts()?.into_iter().map(|a| a.email).collect();
    let iface = s.proxy.bound_interface();
    Ok(Json(check_keyserver(&addrs, iface.as_deref()).await))
}

/// Push one of our stored public keys up to keys.openpgp.org so
/// external clients that don't understand Autocrypt (Proton in
/// particular) can still discover our key when deciding whether to
/// encrypt a reply. Also asks Hagrid to send the verification link
/// to the key's email UIDs — the user has to click that link in
/// their inbox before the key actually becomes retrievable.
async fn publish(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<PublishResult>> {
    // Pull the armored block + the key's email identities. Hagrid's
    // verify step needs to know which addresses we're claiming so
    // it only mails links to those — we can't rely on email UIDs
    // inside the key alone because Hagrid re-parses.
    let armored = s.db.pgp_export_public(id)?;
    let row =
        s.db.pgp_list()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
    let mut addresses: Vec<String> = Vec::new();
    if let Some(primary) = row.primary_email {
        addresses.push(primary);
    }
    // Parse the key one more time so we pick up any secondary UIDs
    // (keys with multiple email identities). parse_public_key_info
    // only returns the primary today — we'll extend this when we
    // surface secondary emails in the UI.

    let iface = s.proxy.bound_interface();
    let result = publish_to_hagrid(&armored, &addresses, iface.as_deref()).await?;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct CanEncryptQuery {
    emails: String,
    /// When false (the default, used for the live "can I encrypt?"
    /// indicator while the user types), only the local keyring is
    /// consulted — no network. WKD / keyserver discovery, which reveals
    /// the recipient address to a third party, runs only when the
    /// caller explicitly asks (`discover=1`): i.e. the user turned
    /// encryption on or hit Send. This keeps a typed-then-deleted
    /// address from leaking to a public keyserver.
    #[serde(default)]
    discover: bool,
}

/// Check whether all supplied recipient emails have a public key
/// available for encryption. For addresses that aren't already in
/// the local keyring, attempt a WKD (and keyserver) lookup in
/// parallel — if discovery succeeds, import the key so future sends
/// skip the network round-trip.
///
/// This is what makes "type a Proton address, encryption auto-
/// enables" work: Proton publishes every user's public key at
/// `openpgpkey.proton.me` (WKD), and we pull it through the VPN
/// tunnel the first time the address appears in the compose form.
///
/// Returned shape:
///   - `can_encrypt`: true when every non-empty address now has a key
///     locally (whether it was there already or we just imported it)
///   - `missing`: addresses we still couldn't find anywhere
///   - `imported`: addresses whose keys we just fetched via WKD /
///     keyserver during this call (so the UI can show a subtle
///     "key discovered for alice@proton.me" hint if it wants to)
async fn can_encrypt(
    State(s): State<AppState>,
    Query(q): Query<CanEncryptQuery>,
) -> Result<Json<serde_json::Value>> {
    // First pass: split + normalize inputs, bucket into already-local
    // vs needs-lookup. Doing this in a blocking loop is fine — it's
    // only a handful of DB point reads.
    let mut needs_lookup: Vec<String> = Vec::new();
    let mut total_addrs = 0usize;
    for raw in q.emails.split(',') {
        let email = raw.trim().to_ascii_lowercase();
        if email.is_empty() {
            continue;
        }
        let addr = if let (Some(lt), Some(gt)) = (email.find('<'), email.rfind('>')) {
            email[lt + 1..gt].trim().to_string()
        } else {
            email
        };
        total_addrs += 1;
        if s.db.pgp_find_by_email(&addr).unwrap_or(None).is_none() {
            needs_lookup.push(addr);
        }
    }

    // Nothing to look up — fast path, skip HTTP entirely.
    if needs_lookup.is_empty() {
        return Ok(Json(serde_json::json!({
            "can_encrypt": total_addrs > 0,
            "missing": Vec::<String>::new(),
            "imported": Vec::<String>::new(),
        })));
    }

    // Some addresses aren't in the keyring. Reaching out to WKD /
    // keyservers to find them discloses the recipient address to a
    // third party — so we only do it when the caller explicitly opts
    // in (`discover=1`). The default keystroke-driven probe stops here
    // and simply reports the addresses as missing, leaking nothing.
    if !q.discover {
        return Ok(Json(serde_json::json!({
            "can_encrypt": false,
            "missing": needs_lookup,
            "imported": Vec::<String>::new(),
        })));
    }

    // Fire all discovery requests in parallel. Each one runs with its
    // own 10s HTTP budget internally, so total latency is bounded by
    // the slowest address rather than the sum. Pass the VPN-bound
    // interface through so the WKD HTTPS fetch routes via wg0 — the
    // default-route socket can't reach the in-tunnel DNS resolver.
    let iface = s.proxy.bound_interface();
    let handles: Vec<_> = needs_lookup
        .iter()
        .cloned()
        .map(|addr| {
            let iface = iface.clone();
            tokio::spawn(async move {
                let res = discover_key(&addr, iface.as_deref()).await;
                (addr, res)
            })
        })
        .collect();

    let mut imported: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for h in handles {
        let Ok((addr, res)) = h.await else { continue };
        match res {
            Ok(DiscoveryResult {
                source,
                armored_public_key: Some(armored),
                ..
            }) if source != DiscoverySource::NotFound => {
                match try_import_discovered(&s, &addr, &armored) {
                    Ok(()) => {
                        tracing::info!(address = %addr, ?source, "auto-discovered recipient key");
                        imported.push(addr);
                    }
                    Err(e) => {
                        tracing::warn!(address = %addr, error = %e, "discovered key failed to import");
                        missing.push(addr);
                    }
                }
            }
            _ => {
                missing.push(addr);
            }
        }
    }

    Ok(Json(serde_json::json!({
        "can_encrypt": total_addrs > 0 && missing.is_empty(),
        "missing": missing,
        "imported": imported,
    })))
}

/// Parse a freshly-discovered WKD key and insert it into the keyring
/// as a public-only row sourced from WKD. Kept in its own helper so
/// the parallel map in `can_encrypt` doesn't get noisy.
fn try_import_discovered(s: &AppState, addr: &str, armored: &str) -> Result<()> {
    let info = parse_public_key_info(armored)
        .map_err(|e| Error::Other(anyhow::anyhow!("parse discovered key for {addr}: {e}")))?;
    s.db.pgp_upsert(
        &NewKey {
            info: &info,
            armored_public: armored,
            armored_secret: None,
            source: KeySource::Wkd,
        },
        &s.vault,
    )?;
    Ok(())
}
