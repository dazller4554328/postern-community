# Email templates

Customer-facing email templates — currently the new-license welcome.

## `postern-pro-welcome.{html,txt}`

Sent automatically when a Postern Pro order is provisioned. Walks the
buyer through the bootstrap install in four steps, with their license
key already merged into the install one-liner so they can copy-paste
straight into a fresh SSH session.

### Importing into WHMCS

1. WHMCS admin → **Setup → Email Templates → Create New Email Template**.
2. **Type**: *Product/Service*. **Unique Name**:
   `postern_pro_welcome` (used by the licensing module's
   `WelcomeEmail` setting).
3. **Subject**:
   ```
   Your Postern Pro license — install in 10 minutes
   ```
4. Paste `postern-pro-welcome.html` into the HTML body and
   `postern-pro-welcome.txt` into the plain-text body.
5. On the Postern Pro product (Setup → Products/Services → Postern Pro
   → Module Settings), set **Welcome Email** to
   `postern_pro_welcome`.

### Merge tags

| Tag | Meaning |
|---|---|
| `{$client_first_name}` | Customer's first name (WHMCS standard) |
| `{$service_product_name}` | "Postern Pro" — pulled from the product (WHMCS standard) |
| `{$service_username}` | The license key — see note below |
| `{$ticket_url}` | Customer's billing-portal / support link |
| `{$signature}` | Admin signature (WHMCS standard) |

### Why `{$service_username}` carries the license key

The Postern WHMCS server module
(`/var/www/postern/billing/modules/servers/postern/postern.php`) mints a
license key on `CreateAccount` and mirrors it into `tblhosting.username`
via `postern_setHostingUsername()`. WHMCS exposes that column as
`{$service_username}` in product/service email templates. So the
welcome email reads the key straight from the row WHMCS already
knows about — no addons, no custom fields.

We do **not** use the WHMCS Licensing Addon, so its
`{$service_license_key}` merge tag is unavailable. Don't substitute
that name back in unless you also install the addon and rewire the
module.

### Testing on an existing service

If you trigger a test send against a service that pre-dates the
Postern module, `tblhosting.username` may be empty (the module's
`CreateAccount` only fires on new orders, not on services that
already existed when the module was attached). Two ways to backfill:

```sql
-- Verify the service has a license row:
SELECT h.id, h.username, l.license_key
FROM tblhosting h
LEFT JOIN postern_licenses l ON l.service_id = h.id
WHERE h.id = <SERVICE_ID>;

-- If license_key is set but username is empty, mirror it:
UPDATE tblhosting h
JOIN postern_licenses l ON l.service_id = h.id
SET h.username = l.license_key
WHERE h.id = <SERVICE_ID>;
```

Or in WHMCS admin, run **Module Commands → Run Create** on the
service — `postern_CreateAccount` is idempotent (it re-mirrors the
existing license to `username` if a `postern_licenses` row already
exists).

### Updating

These templates are the source of truth. WHMCS keeps its own copy
in the database — to push changes, re-paste the bodies under
*Setup → Email Templates → postern_pro_welcome*. Or drive it from the
WHMCS API (`SendAdminEmail` doesn't update template bodies, but you
can export/import via SQL on the `tblemailtemplates` table).

### Tone

Match the docs: terse, technical, no marketing copy. The customer
just paid, they don't need to be sold to — they need to be unblocked.
Keep the install path the same as `docs/install/index.md` so the
two never disagree.
