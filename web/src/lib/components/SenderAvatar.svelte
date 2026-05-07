<script lang="ts">
  // Sender chip. Resolves a real avatar via /api/avatar (Gravatar →
  // domain favicon, proxied through the VPN-aware update client) and
  // falls back to deterministic colour-coded initials when nothing
  // remote is found. The proxy returns 404 instead of a robohash
  // default so a "no avatar registered" sender silently lands on the
  // initials chip rather than a generic placeholder.
  //
  // Privacy note: a successful Gravatar lookup leaks "this sender is
  // being viewed" to gravatar.com (a Cloudflare property). The proxy
  // rides the kill-switched egress when a VPN is engaged. For users
  // who'd rather skip the lookup entirely, pass `fetchRemote={false}`.

  interface Props {
    email: string | null | undefined;
    /** Pixel size of the avatar square. */
    size?: number;
    /**
     * When true, attempt to fetch a real avatar via the server-side
     * proxy. False = always show initials, no network. Defaults to
     * true for the inbox row where remote avatars carry the most
     * scanability value.
     */
    fetchRemote?: boolean;
  }

  let { email, size = 28, fetchRemote = true }: Props = $props();

  // Cache across all SenderAvatar instances in the app. Lets a long
  // thread with many messages from the same sender re-use one fetch.
  // Map value: URL string (resolved), null (confirmed no remote
  // avatar — render initials), or undefined (not yet resolved).
  type CachedAvatar = string | null;
  const AVATAR_CACHE = ((globalThis as unknown) as { __posternAvatarCache?: Map<string, CachedAvatar> })
    .__posternAvatarCache ??= new Map();

  let normalized = $derived.by(() => {
    if (!email) return '';
    const raw = email.includes('<') && email.includes('>')
      ? email.slice(email.indexOf('<') + 1, email.indexOf('>'))
      : email;
    return raw.trim().toLowerCase();
  });

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
    // Probe via the proxy. We don't pre-flight with fetch() — let the
    // <img> tag's onload/onerror tell us whether the URL resolved.
    // Saves a round trip when the avatar succeeds (the common case
    // for known senders).
    remoteUrl = `/api/avatar?email=${encodeURIComponent(addr)}&size=${size * 2}`;
  });

  function onLoad() {
    if (normalized && remoteUrl) {
      AVATAR_CACHE.set(normalized, remoteUrl);
    }
  }
  function onError() {
    // Either Gravatar + favicon both missed, or the VPN tunnel isn't
    // up. Cache the null so we don't retry on every render.
    if (normalized) {
      AVATAR_CACHE.set(normalized, null);
    }
    remoteUrl = null;
  }

  let initials = $derived.by(() => {
    const addr = normalized;
    if (!addr) return '?';
    const local = addr.split('@')[0] ?? addr;
    const parts = local
      .split(/[\s._\-+]+/)
      .filter((p) => p.length > 0)
      .slice(0, 2);
    if (parts.length === 0) return (local[0] ?? '?').toUpperCase();
    return parts.map((p) => p[0]?.toUpperCase() ?? '').join('');
  });

  // Same palette as ACCOUNT_COLOR_PALETTE in $lib/accountColor — kept
  // duplicated so a contact-page chip and an inbox-row pill read as
  // the same visual language without forcing a cross-import.
  const PALETTE = [
    '#3b82f6',
    '#10b981',
    '#f59e0b',
    '#ef4444',
    '#8b5cf6',
    '#ec4899',
    '#14b8a6',
    '#6366f1',
    '#84cc16',
    '#f97316'
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
