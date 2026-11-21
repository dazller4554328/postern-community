<script lang="ts">
  import {
    api,
    type BackupDestination,
    type GDrivePublicConfig,
    type SftpPublicConfig
  } from '$lib/api';
  import { formatDate } from '$lib/format';

  interface Props {
    destination: BackupDestination;
    /** Parent re-fetches the destinations list when state changes
     *  (toggle, push, forget fingerprint, remove). */
    onChanged: () => Promise<void> | void;
  }

  let { destination, onChanged }: Props = $props();

  // Per-card transient state. The parent used to track these in
  // {[id]: ...} dictionaries; owning them per-component keeps the
  // shape simpler.
  let busy = $state(false);
  let flash = $state('');

  function asSftp(d: BackupDestination): SftpPublicConfig {
    return d.public_config as SftpPublicConfig;
  }
  function asGdrive(d: BackupDestination): GDrivePublicConfig {
    return d.public_config as GDrivePublicConfig;
  }

  // Local copy of the parent's helper. Not pulled into $lib/format
  // because the GB tier matters here (backup tarballs run hundreds
  // of MB+) and that diverges from format.ts's MB-cap shape.
  function humanSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function toggleEnabled() {
    busy = true;
    try {
      await api.updateBackupDestination(destination.id, { enabled: !destination.enabled });
      await onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      busy = false;
    }
  }

  async function test() {
    busy = true;
    flash = '';
    try {
      const r = await api.testBackupDestination(destination.id);
      // Response shape branches on kind. SFTP carries TOFU fingerprint
      // info; GDrive carries the verified folder + account.
      if (destination.kind === 'sftp' && r.fingerprint) {
        const fpShort = r.fingerprint.length > 30
          ? r.fingerprint.slice(0, 30) + '…'
          : r.fingerprint;
        flash = r.first_use
          ? `✓ connected. Pinned hostkey ${fpShort} for future connects.`
          : `✓ connected. Hostkey verified (${fpShort}).`;
        if (r.first_use) await onChanged();
      } else if (destination.kind === 'gdrive') {
        flash = `✓ connected. Drive folder "${r.folder_name ?? 'Postern Backups'}" reachable.`;
      } else {
        flash = '✓ connected.';
      }
    } catch (e) {
      flash = '⚠ ' + (e instanceof Error ? e.message : String(e));
    } finally {
      busy = false;
    }
  }

  async function forgetFingerprint() {
    if (
      !confirm(
        `Forget the pinned hostkey for "${destination.label}"? The next connect will accept ` +
          `whatever key the server presents and re-pin. Only do this if you ` +
          `legitimately rotated the SSH host key on the destination box.`
      )
    ) return;
    busy = true;
    try {
      await api.forgetBackupDestinationFingerprint(destination.id);
      flash = '↻ fingerprint forgotten — next test/push will TOFU-pin a new key';
      await onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      busy = false;
    }
  }

  async function pushLatest() {
    if (!confirm(`Push the most recent backup to ${destination.label}?`)) return;
    busy = true;
    flash = '';
    try {
      const r = await api.pushBackupDestination(destination.id);
      flash = `✓ pushed ${humanSize(r.bytes_uploaded)} → ${r.remote_path}`;
      await onChanged();
    } catch (e) {
      flash = '⚠ ' + (e instanceof Error ? e.message : String(e));
      await onChanged();
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!confirm(`Remove destination "${destination.label}"? This deletes the credentials too.`)) return;
    try {
      await api.deleteBackupDestination(destination.id);
      await onChanged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
  }
</script>

<li class="dest-item" class:disabled={!destination.enabled}>
  <div class="dest-id">
    <strong>{destination.label}</strong>
    {#if destination.kind === 'sftp'}
      {@const sftp = asSftp(destination)}
      <span class="muted">
        sftp://{sftp.username}@{sftp.host}:{sftp.port}{sftp.remote_dir}
      </span>
      {#if destination.server_fingerprint}
        <span class="fp pinned" title="Pinned via TOFU. Future connects must present this exact key.">
          🔒 {destination.server_fingerprint}
        </span>
      {:else}
        <span class="fp unpinned" title="No hostkey pinned. The next connect will accept any key and pin it.">
          ⚠ no hostkey pinned (TOFU on next connect)
        </span>
      {/if}
    {:else if destination.kind === 'gdrive'}
      {@const g = asGdrive(destination)}
      <span class="muted">
        Google Drive{#if g.account_email} · {g.account_email}{/if} · {g.folder_name}
      </span>
    {/if}
  </div>
  <div class="dest-state">
    {#if destination.last_push_status === 'ok'}
      <span class="status-ok">
        ✓ pushed {destination.last_push_filename} {destination.last_push_at ? formatDate(destination.last_push_at) : ''}
      </span>
    {:else if destination.last_push_status === 'error'}
      <details class="err-details">
        <summary class="status-err">
          ⚠ last push failed: {destination.last_push_error?.slice(0, 80) ?? 'unknown'}{(destination.last_push_error?.length ?? 0) > 80 ? '…' : ''}
        </summary>
        <textarea
          class="err-full"
          readonly
          rows={Math.min(8, Math.max(2, ((destination.last_push_error ?? '').match(/\n/g)?.length ?? 0) + 2))}
        >{destination.last_push_error ?? ''}</textarea>
      </details>
    {:else}
      <span class="muted">no pushes yet</span>
    {/if}
    {#if flash}
      <div class="dest-flash">{flash}</div>
    {/if}
  </div>
  <div class="dest-actions">
    <button class="row-btn" onclick={test} disabled={busy}>Test</button>
    <button class="row-btn" onclick={pushLatest} disabled={busy}>
      Push latest
    </button>
    <button class="row-btn" onclick={toggleEnabled} disabled={busy}>
      {destination.enabled ? 'Disable' : 'Enable'}
    </button>
    {#if destination.kind === 'sftp' && destination.server_fingerprint}
      <button class="row-btn" onclick={forgetFingerprint} disabled={busy}
              title="Clear pinned hostkey (only after a deliberate key rotation)">
        Forget hostkey
      </button>
    {/if}
    <button class="del" onclick={remove}>Remove</button>
  </div>
</li>

<style>
  /* Shared classes (.row-btn, .del, .muted) live in +page.svelte
     under :global() so this child inherits them. Card-specific
     layout + status colours are scoped here. */
  .dest-item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.85rem;
    padding: 0.7rem 0.85rem;
    border: 1px solid var(--border); border-radius: 0.5rem;
    margin-bottom: 0.5rem;
    background: var(--surface);
  }
  .dest-item.disabled { opacity: 0.55; }
  .dest-id strong { font-size: 0.88rem; }
  .dest-id .muted { display: block; font-size: 0.74rem; margin-top: 0.15rem; opacity: 0.6; }
  .fp {
    display: block;
    margin-top: 0.2rem;
    font-family: ui-monospace, monospace;
    font-size: 0.7rem;
    word-break: break-all;
  }
  .fp.pinned { color: forestgreen; }
  .fp.unpinned { color: color-mix(in oklab, goldenrod 80%, currentColor); font-family: inherit; }
  .dest-state { font-size: 0.78rem; min-width: 0; }
  .dest-state .status-ok { color: forestgreen; }
  .dest-state .status-err { color: color-mix(in oklab, crimson 70%, currentColor); }
  .err-details { font-size: 0.78rem; }
  .err-details summary { cursor: pointer; list-style: revert; }
  .err-details .err-full {
    width: 100%; margin-top: 0.4rem; box-sizing: border-box;
    font: inherit; font-size: 0.74rem;
    font-family: ui-monospace, monospace;
    padding: 0.4rem 0.55rem;
    background: var(--surface-2, rgba(255,255,255,0.04));
    color: inherit;
    border: 1px solid var(--border, rgba(255,255,255,0.1));
    border-radius: 0.3rem;
    resize: vertical;
  }
  .dest-flash { margin-top: 0.25rem; font-size: 0.78rem; opacity: 0.85; }
  .dest-actions { display: flex; gap: 0.35rem; flex-wrap: wrap; justify-content: flex-end; }

  @media (max-width: 720px) {
    .dest-item { grid-template-columns: 1fr; }
    .dest-actions { justify-content: flex-start; }
  }
</style>
