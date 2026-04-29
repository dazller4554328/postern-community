<script lang="ts">
  interface Props {
    email: string | null | undefined;
    /** Pixel size of the avatar square. */
    size?: number;
    /**
     * When true, attempt to fetch a real avatar (Gravatar → domain
     * favicon) via the server-side proxy. The proxy routes through
     * the VPN, caches the result, and returns 404 if nothing was
     * found — in which case we fall back to initials. When false,
     * initials are always shown; no network.
     */
    fetchRemote?: boolean;
  }

  let { email, size = 28, fetchRemote = true }: Props = $props();

  // Cache across all SenderAvatar instances in the app. Lets a long
  // thread with many messages from the same sender re-use one fetch.
  // Map value is either the URL string (resolved), null (confirmed
  // no remote avatar), or undefined (not yet resolved).
  type CachedAvatar = string | null;
  const AVATAR_CACHE = ((globalThis as unknown) as { __posternAvatarCache?: Map<string, CachedAvatar> })
    .__posternAvatarCache ??= new Map();

  let normalized = $derived.by(() => {
    if (!email) return '';
    // Strip "Name <addr>" → "addr"
    const raw = email.includes('<') && email.includes('>')
      ? email.slice(email.indexOf('<') + 1, email.indexOf('>'))
      : email;
    return raw.trim().toLowerCase();
  });

  // Visual content: until we know whether a remote avatar exists we
  // show initials. Flip to the remote URL once resolved. On 404, stay
  // on initials.
  let remoteUrl = $state<CachedAvatar | undefined>(undefined);
  $effect(() => {
    const addr = normalized;
    if (!addr || !fetchRemote) {
      remoteUrl = null;
      return;
    }
    const cached = AVATAR_CACHE.get(addr);
    if (cached !== undefined) {
      remoteUrl = cached;
      return;
    }
    // Kick off the lookup. We only trust a successful load — handle
    // failures (404, network error) in the <img>'s onerror. This
    // avoids an extra fetch() round-trip just to probe.
    const probeUrl = `/api/avatar?email=${encodeURIComponent(addr)}&size=${size * 2}`;
    remoteUrl = probeUrl;
  });

  function onLoad() {
    // Image decoded successfully → remember the working URL.
    if (normalized && remoteUrl) {
      AVATAR_CACHE.set(normalized, remoteUrl);
    }
  }
  function onError() {
    // Either Gravatar + favicon both missed, or the VPN tunnel
    // isn't up. Cache the null so we don't retry on every render.
    if (normalized) {
      AVATAR_CACHE.set(normalized, null);
    }
    remoteUrl = null;
  }

  let initials = $derived.by(() => {
    const addr = normalized;
    if (!addr) return '?';
    const local = addr.split('@')[0] ?? addr;
    // Grab the first letter of the first two word-ish segments of
    // the local part ("john.doe" → "JD", "alice" → "A").
    const parts = local
      .split(/[\s._\-+]+/)
      .filter((p) => p.length > 0)
      .slice(0, 2);
    if (parts.length === 0) return (local[0] ?? '?').toUpperCase();
    return parts.map((p) => p[0]?.toUpperCase() ?? '').join('');
  });

  // Deterministic color per email so "bob@x.com" always gets the
  // same chip. Pick from a small palette tuned to work on both
  // light and dark backgrounds.
  const PALETTE = [
    '#3b82f6', // blue
    '#10b981', // emerald
    '#f59e0b', // amber
    '#ef4444', // red
    '#8b5cf6', // violet
    '#ec4899', // pink
    '#14b8a6', // teal
    '#6366f1', // indigo
    '#84cc16', // lime
    '#f97316' // orange
  ];
  let bgColor = $derived.by(() => {
    const addr = normalized;
    if (!addr) return PALETTE[0];
    let h = 0;
    for (let i = 0; i < addr.length; i++) h = (h * 31 + addr.charCodeAt(i)) >>> 0;
    return PALETTE[h % PALETTE.length];
  });
</script>

<span
  class="avatar"
  style:width="{size}px"
  style:height="{size}px"
  style:background-color={remoteUrl ? 'transparent' : bgColor}
  style:font-size="{Math.max(10, Math.round(size * 0.38))}px"
  title={email ?? undefined}
>
  {#if remoteUrl}
    <img
      src={remoteUrl}
      alt=""
      loading="lazy"
      onload={onLoad}
      onerror={onError}
      style:width="{size}px"
      style:height="{size}px"
    />
  {:else}
    <span class="initials">{initials}</span>
  {/if}
</span>

<style>
  .avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border-radius: 50%;
    overflow: hidden;
    color: #ffffff;
    font-weight: 600;
    line-height: 1;
    letter-spacing: 0.01em;
    user-select: none;
    /* Subtle inner highlight so the chip reads as a surface, not a
       flat color sticker. */
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.14);
  }
  .avatar img {
    display: block;
    object-fit: cover;
    width: 100%;
    height: 100%;
  }
  .initials {
    text-transform: uppercase;
  }
</style>
