<script lang="ts">
  import { page } from '$app/stores';

  function isSettingsActive(): boolean {
    return $page.url.pathname.startsWith('/settings')
      && !$page.url.pathname.startsWith('/settings/audit');
  }
</script>

<footer>
  <a
    class="row tool-row footer-settings"
    class:active={isSettingsActive()}
    href="/settings"
    data-sveltekit-preload-code="off"
    data-sveltekit-preload-data="off"
    title="Appearance, mailboxes, privacy, updates"
  >
    <span class="tool-icon" aria-hidden="true">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="10" cy="10" r="2.4" />
        <path d="M16.2 11.8a6.6 6.6 0 0 0 0-3.6l1.7-1.3-1.6-2.7-2 .8a6.6 6.6 0 0 0-3.1-1.8l-.3-2.1h-3.1l-.3 2.1a6.6 6.6 0 0 0-3.1 1.8l-2-.8L1 6.9l1.7 1.3a6.6 6.6 0 0 0 0 3.6L1 13.1l1.6 2.7 2-.8a6.6 6.6 0 0 0 3.1 1.8l.3 2.1h3.1l.3-2.1a6.6 6.6 0 0 0 3.1-1.8l2 .8L18 13.1Z"/>
      </svg>
    </span>
    <span class="label">Settings</span>
  </a>
</footer>

<style>
  /* Settings link mirrors SidebarTools' visual language (bordered row,
     themed glyph) so the footer reads as a cousin of the Tools section
     rather than a bare link pinned to the bottom. */
  footer {
    padding: 0.55rem 0.55rem 0.75rem;
    border-top: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    display: block;
    min-width: 0;
    overflow: hidden;
    box-sizing: border-box;
  }
  .tool-row {
    font-weight: 500;
    text-decoration: none;
    color: inherit;
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    background: color-mix(in oklab, var(--surface) 92%, transparent);
  }
  .tool-row:hover {
    background: var(--row-hover);
    border-color: color-mix(in oklab, currentColor 14%, transparent);
  }
  .tool-row.active {
    background: var(--row-selected);
    border-color: color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .tool-icon {
    width: 16px;
    height: 16px;
    display: inline-grid;
    place-items: center;
    color: color-mix(in oklab, currentColor 78%, var(--accent));
  }
  .tool-icon svg {
    width: 16px;
    height: 16px;
    display: block;
  }
  .tool-row.active .tool-icon,
  .tool-row:hover .tool-icon {
    color: var(--accent);
  }
  .footer-settings {
    width: 100%;
    margin: 0;
    box-sizing: border-box;
  }
</style>
