<script lang="ts">
  // Evolution/Thunderbird-style folder icons. Map by IMAP folder name.
  // Keep line-weight stroke style so icons sit next to text without
  // overpowering it, and use currentColor so theme just works.
  import { onMount } from 'svelte';
  import { prefs, type Theme } from '$lib/prefs';

  interface Props {
    name: string;
    kind?: 'system' | 'gmail_category' | 'user';
  }

  let { name, kind = 'user' }: Props = $props();
  let currentTheme = $state<Theme>('system');

  function iconFor(n: string, k: string): string {
    const s = n.replace(/^\[Gmail\]\//, '').toLowerCase();
    if (k === 'gmail_category') {
      if (s.includes('social')) return 'social';
      if (s.includes('promo')) return 'tag';
      if (s.includes('update')) return 'bell';
      if (s.includes('forum')) return 'chat';
      return 'tag';
    }
    if (s === 'inbox') return 'inbox';
    if (s === 'starred') return 'star';
    if (s === 'important') return 'important';
    if (s === 'sent' || s === 'sent mail') return 'sent';
    if (s === 'drafts') return 'draft';
    if (s === 'spam' || s === 'junk') return 'spam';
    if (s === 'trash' || s === 'bin') return 'trash';
    if (s === 'archive' || s === 'all mail') return 'archive';
    return 'folder';
  }

  let kind_icon = $derived(iconFor(name, kind));
  let cyberpunkTheme = $derived(currentTheme === 'cyberpunk');

  onMount(() => {
    const unsub = prefs.subscribe((p) => {
      currentTheme = p.theme;
    });
    return unsub;
  });
</script>

<span class="folder-icon" aria-hidden="true">
  {#if cyberpunkTheme && kind_icon === 'inbox'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round">
      <path d="M3 3.5h10l1.2 4v3.8L12.8 13H3.2L1.8 11.3V7.5L3 3.5Z"/>
      <path d="M2.4 8.3h3l1 1.5h3.2l1-1.5h3"/>
      <path d="M5.2 5.4h5.6"/>
    </svg>
  {:else if kind_icon === 'inbox'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round">
      <path d="M2 9.5 4 3h8l2 6.5v3a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-3Z"/>
      <path d="M2 9.5h3l1 2h4l1-2h3"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'star'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="m8 1.8 1.5 4 4.2 1.5-4.2 1.5-1.5 4-1.5-4-4.2-1.5 4.2-1.5Z"/>
      <path d="M8 4.6v5.1"/>
      <path d="M5.9 7.1h4.2"/>
    </svg>
  {:else if kind_icon === 'star'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="m8 2 1.8 3.7 4 .6-2.9 2.8.7 4L8 11.2 4.4 13.1l.7-4L2.2 6.3l4-.6Z"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'important'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M8 1.7 12.8 4v4.2c0 2.6-1.9 4.3-4.8 6.1-2.9-1.8-4.8-3.5-4.8-6.1V4L8 1.7Z"/>
      <path d="M8 5v4.2"/>
      <path d="M8 11.5h.01"/>
    </svg>
  {:else if kind_icon === 'important'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.5 3 8 1.5 13.5 3v3.5c0 3-2.2 5.8-5.5 7-3.3-1.2-5.5-4-5.5-7V3Z"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'sent'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.2 8 13.8 2.6 10.1 13.6 7.4 9.8 2.2 8Z"/>
      <path d="M7.3 9.6 13.8 2.6"/>
    </svg>
  {:else if kind_icon === 'sent'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="m2 8 12-5.5L9 14l-2-5.5L2 8Z"/>
      <path d="M7 8.5 14 2.5"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'draft'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M11.7 2.2 13.8 4.3 6.3 11.8 3.2 12.8l1-3.1 7.5-7.5Z"/>
      <path d="M10.4 3.5 12.5 5.6"/>
      <path d="M3.6 12.4h3"/>
    </svg>
  {:else if kind_icon === 'draft'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M11.5 2.5 13 4l-7 7-2 .5.5-2 7-7Z"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'spam'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M8 1.8 14 12.8H2L8 1.8Z"/>
      <path d="M8 5.1v3.5"/>
      <path d="M8 10.9h.01"/>
      <path d="M5.6 12.1h4.8"/>
    </svg>
  {:else if kind_icon === 'spam'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M8 1.5 14.5 13H1.5L8 1.5Z"/>
      <path d="M8 6.5v3"/><path d="M8 11.2v.1"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'trash'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M3 4.1h10"/>
      <path d="M5.2 4.1V2.8h5.6v1.3"/>
      <path d="M4.1 4.1 4.8 13h6.4l.7-8.9"/>
      <path d="M6.5 6.3v4.7M9.5 6.3v4.7"/>
    </svg>
  {:else if kind_icon === 'trash'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.5 4.5h11"/>
      <path d="M6 4.5V3a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5"/>
      <path d="M4 4.5v8.5a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.5"/>
      <path d="M7 7v4M9 7v4"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'archive'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.2 3.2h11.6v2.4H2.2z"/>
      <path d="M3.5 5.8V13h9V5.8"/>
      <path d="M6 8.3h4"/>
      <path d="M6.8 10.3h2.4"/>
    </svg>
  {:else if kind_icon === 'archive'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M1.5 3.5h13v2.5h-13z"/>
      <path d="M3 6v7.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6"/>
      <path d="M6.5 9h3"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'social'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <circle cx="4.2" cy="6.2" r="1.5"/>
      <circle cx="11.8" cy="4.8" r="1.5"/>
      <circle cx="8.2" cy="11.2" r="1.5"/>
      <path d="M5.5 6 10.5 5"/>
      <path d="M5 7.2 7.4 10.1"/>
      <path d="M10.8 6 8.8 10"/>
    </svg>
  {:else if kind_icon === 'social'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <circle cx="8" cy="5" r="2.2"/>
      <path d="M3 14c.5-2.5 2.6-4 5-4s4.5 1.5 5 4"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'bell'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M4.4 10.8V7.3A3.6 3.6 0 0 1 8 3.7a3.6 3.6 0 0 1 3.6 3.6v3.5l1.3 1.4H3.1l1.3-1.4Z"/>
      <path d="M6.8 13a1.3 1.3 0 0 0 2.4 0"/>
      <path d="M8 2.3v1"/>
    </svg>
  {:else if kind_icon === 'bell'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M4 11V7a4 4 0 0 1 8 0v4l1 1.5H3L4 11Z"/>
      <path d="M6.8 14a1.3 1.3 0 0 0 2.4 0"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'chat'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.4 3.3h11.2v6.9H7.2l-3 2v-2H2.4V3.3Z"/>
      <path d="M5.1 6.1h5.8"/>
      <path d="M5.1 8h3.8"/>
    </svg>
  {:else if kind_icon === 'chat'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.5 3.5h11v7H6l-3 2.5v-2.5H2.5V3.5Z"/>
    </svg>
  {:else if cyberpunkTheme && kind_icon === 'tag'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.5 8.2V3.1h5.1l5.9 5.9-5.2 5.2-5.8-6Z"/>
      <path d="M5.1 5.7h.01"/>
      <path d="M7.6 3.2v2.6h2.6"/>
    </svg>
  {:else if kind_icon === 'tag'}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2 8.5V3a1 1 0 0 1 1-1h5.5L14 7.5 8.5 13 2 8.5Z"/>
      <circle cx="5.3" cy="5.3" r=".9" fill="currentColor"/>
    </svg>
  {:else if cyberpunkTheme}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.15" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2.2 5V3.5h4L7.5 2h6.3v10.6L12.8 14H3.2L2.2 12.6V5Z"/>
      <path d="M2.2 5h11.6"/>
      <path d="M5.2 8.2h5.6"/>
    </svg>
  {:else}
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" stroke-linecap="round">
      <path d="M2 5.5V13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V5a1 1 0 0 0-1-1H7.5L6 2.5H3a1 1 0 0 0-1 1v2Z"/>
    </svg>
  {/if}
</span>

<style>
  .folder-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    opacity: 0.75;
  }
  :global(html[data-theme='cyberpunk']) .folder-icon {
    opacity: 0.92;
    filter:
      drop-shadow(0 0 4px rgba(122, 162, 247, 0.26))
      drop-shadow(0 0 10px rgba(187, 154, 247, 0.1));
  }
</style>
