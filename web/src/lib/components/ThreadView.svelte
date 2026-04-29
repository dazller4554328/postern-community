<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type MessageListItem } from '$lib/api';
  import { formatDate, formatSender } from '$lib/format';
  import MessageBody from './MessageBody.svelte';

  interface Props {
    threadId: string;
  }
  let { threadId }: Props = $props();

  let messages = $state<MessageListItem[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);
  // Expanded state per message — latest expanded by default.
  let expanded = $state<Record<number, boolean>>({});

  async function load() {
    loading = true;
    err = null;
    try {
      messages = await api.threadMessages(threadId);
      const latest = messages[messages.length - 1];
      if (latest) expanded = { [latest.id]: true };
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    threadId;
    load();
  });

  function toggle(id: number) {
    expanded = { ...expanded, [id]: !expanded[id] };
  }

  let subject = $derived(messages[messages.length - 1]?.subject || '(no subject)');
</script>

<section>
  {#if loading}
    <p class="placeholder">Loading conversation…</p>
  {:else if err}
    <p class="err">Error: {err}</p>
  {:else if messages.length === 0}
    <p class="placeholder">(empty thread)</p>
  {:else}
    <header class="thread-header">
      <h1>{subject}</h1>
      <div class="meta">{messages.length} message{messages.length === 1 ? '' : 's'}</div>
    </header>

    <ul class="stack">
      {#each messages as m (m.id)}
        <li class:unread={!m.is_read} class:expanded={expanded[m.id]}>
          <button class="head" onclick={() => toggle(m.id)}>
            <span class="sender">{formatSender(m.from_addr)}</span>
            <span class="excerpt">{m.snippet ?? ''}</span>
            <span class="date">{formatDate(m.date_utc)}</span>
          </button>
          {#if expanded[m.id]}
            <div class="body">
              <MessageBody messageId={m.id} variant="preview" />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section {
    height: 100%;
    overflow: auto;
    padding: 1rem 1.1rem 1.35rem;
  }
  .placeholder {
    opacity: 0.55;
    padding: 2rem;
    text-align: center;
  }
  .err {
    color: #c83333;
    padding: 2rem;
    text-align: center;
  }

  .thread-header {
    padding-bottom: 0.9rem;
    margin-bottom: 1rem;
    border-bottom: 1px solid var(--border);
  }
  .thread-header h1 {
    margin: 0 0 0.3rem;
    font-size: 1.25rem;
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  .thread-header .meta {
    font-size: 0.78rem;
    opacity: 0.55;
  }

  ul.stack {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  ul.stack li {
    border: 1px solid var(--border);
    border-radius: 1rem;
    background: var(--surface);
    overflow: hidden;
    box-shadow: 0 10px 22px rgba(0, 0, 0, 0.05);
  }
  ul.stack li.unread {
    border-left: 3px solid var(--accent);
  }

  .head {
    display: grid;
    grid-template-columns: 10rem 1fr 7rem;
    gap: 0.75rem;
    align-items: center;
    width: 100%;
    padding: 0.75rem 0.9rem;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .head:hover {
    background: color-mix(in oklab, currentColor 5%, transparent);
  }
  li.expanded .head {
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 72%, transparent);
  }

  .sender {
    font-size: 0.85rem;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li.unread .sender {
    font-weight: 700;
  }
  .excerpt {
    opacity: 0.55;
    font-size: 0.82rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .date {
    text-align: right;
    font-size: 0.73rem;
    opacity: 0.55;
  }

  .body {
    padding: 0;
  }
  .body :global(section.preview) {
    padding: 0.95rem 1rem 1.15rem;
  }

  @media (max-width: 900px) {
    section {
      padding: 0.85rem 0.8rem 1rem;
    }
    .thread-header {
      padding-bottom: 0.75rem;
      margin-bottom: 0.8rem;
    }
    .thread-header h1 {
      font-size: 1.05rem;
      line-height: 1.35;
    }
    .head {
      grid-template-columns: minmax(0, 1fr) auto;
      grid-template-areas:
        'sender date'
        'excerpt excerpt';
      gap: 0.35rem 0.65rem;
      padding: 0.75rem 0.8rem;
      align-items: start;
    }
    .sender {
      grid-area: sender;
      white-space: normal;
      line-height: 1.3;
    }
    .excerpt {
      grid-area: excerpt;
      white-space: normal;
      line-height: 1.35;
      overflow-wrap: anywhere;
    }
    .date {
      grid-area: date;
      white-space: nowrap;
      font-size: 0.68rem;
    }
    .body :global(section.preview) {
      padding: 0.8rem 0.8rem 1rem;
    }
  }
</style>
