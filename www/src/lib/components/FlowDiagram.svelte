<script lang="ts">
  import Icon from './Icon.svelte';

  type Tone = 'private' | 'public';
  interface Node {
    icon: string;
    title: string;
    sub: string;
    tone?: Tone;
  }
  interface Link {
    label: string;
    tone: Tone;
    lock?: boolean;
  }
  interface Mode {
    label: string;
    tag: string;
    summary: string;
    nodes: Node[];
    links: Link[];
  }

  const MODES: Record<'community' | 'pro', Mode> = {
    community: {
      label: 'Community',
      tag: 'Free · localhost',
      summary:
        'Postern runs on the machine you use it from. Your keys and the encrypted vault stay on that device — mail still sends and receives through your own provider.',
      nodes: [
        { icon: 'laptop', title: 'You', sub: 'Browser on your PC' },
        { icon: 'shield', title: 'Postern', sub: 'Runs locally · vault on disk', tone: 'private' },
        { icon: 'globe', title: 'Internet', sub: 'Public route' },
        { icon: 'server', title: 'Gmail / IMAP', sub: 'Your mail provider' },
        { icon: 'mail', title: 'Recipient', sub: 'Their inbox' }
      ],
      links: [
        { label: 'on-device', tone: 'private', lock: true },
        { label: 'SMTP / IMAP', tone: 'public' },
        { label: 'TLS', tone: 'public' },
        { label: 'delivered', tone: 'public' }
      ]
    },
    pro: {
      label: 'Pro',
      tag: 'Self-hosted VPS',
      summary:
        'Postern lives on your own VPS. You reach it from any device over the Tailscale mesh, and its outbound mail exits through a VPN with a kill-switch — your provider never sees your real IP, and nothing is stored on the device in your hand.',
      nodes: [
        { icon: 'phone', title: 'Your devices', sub: 'Phone, laptop, anywhere' },
        { icon: 'server', title: 'Your VPS', sub: 'Postern + encrypted vault', tone: 'private' },
        { icon: 'globe', title: 'Internet', sub: 'Via VPN egress' },
        { icon: 'server', title: 'Gmail / IMAP', sub: 'Your mail provider' },
        { icon: 'mail', title: 'Recipient', sub: 'Their inbox' }
      ],
      links: [
        { label: 'Tailscale · encrypted', tone: 'private', lock: true },
        { label: 'VPN · kill-switch', tone: 'private', lock: true },
        { label: 'TLS', tone: 'public' },
        { label: 'delivered', tone: 'public' }
      ]
    }
  };

  let mode = $state<'community' | 'pro'>('community');
  let active = $derived(MODES[mode]);
</script>

<div class="flow card">
  <div class="flow__top">
    <div class="seg" role="tablist" aria-label="Deployment topology">
      <button
        role="tab"
        aria-selected={mode === 'community'}
        class:on={mode === 'community'}
        onclick={() => (mode = 'community')}>Community</button
      >
      <button
        role="tab"
        aria-selected={mode === 'pro'}
        class:on={mode === 'pro'}
        onclick={() => (mode = 'pro')}>Pro</button
      >
    </div>
    <span class="flow__tag">{active.tag}</span>
  </div>

  <!-- keyed so a mode switch re-triggers the entrance + flow animation -->
  {#key mode}
    <div class="diagram">
      {#each active.nodes as node, i}
        <div class="node" class:private={node.tone === 'private'} style="--d:{i * 90}ms">
          <span class="node__ico"><Icon name={node.icon} size={24} /></span>
          <span class="node__title">{node.title}</span>
          <span class="node__sub">{node.sub}</span>
        </div>
        {#if i < active.links.length}
          {@const link = active.links[i]}
          <div class="link {link.tone}" style="--d:{i * 90 + 45}ms">
            <span class="link__label">
              {#if link.lock}<Icon name="lock" size={11} />{/if}
              {link.label}
            </span>
            <span class="track"><span class="packet"></span></span>
          </div>
        {/if}
      {/each}
    </div>
  {/key}

  <p class="flow__summary">{active.summary}</p>

  <!-- Autocrypt is topology-independent — shown for both editions. -->
  <figure class="wire">
    <figcaption><Icon name="key" size={13} /> What rides on the wire</figcaption>
    <pre class="wire__code"><span class="dim">From: you@your-domain.com
To: friend@example.com</span>
<span class="ac">Autocrypt:</span> addr=you@your-domain.com; prefer-encrypt=mutual;
          keydata=mDMEY8f1k2gB…AB  <span class="cmt">← your public key, attached automatically</span>

<span class="pgp">-----BEGIN PGP MESSAGE-----</span>  <span class="cmt">← body the provider can't read</span></pre>
  </figure>
  <p class="wire__note">
    Every message Postern sends carries an <strong>Autocrypt</strong> header — your public key,
    attached automatically. Once two Autocrypt-aware clients have swapped headers, replies encrypt
    <strong>end-to-end</strong> on their own. Your provider relays only ciphertext and routing
    headers — it never sees the body.
  </p>

  <ul class="flow__legend">
    <li><span class="dot private"></span> Encrypted / private hop</li>
    <li><span class="dot public"></span> Public route (TLS in transit)</li>
  </ul>
</div>

<style>
  .flow {
    padding: clamp(1.4rem, 1rem + 2vw, 2.4rem);
  }
  .flow__top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 2rem;
  }
  .seg {
    display: inline-flex;
    gap: 0.25rem;
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    background: var(--bg);
  }
  .seg button {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--text-sm);
    padding: 0.45rem 1.3rem;
    border-radius: var(--r-pill);
    cursor: pointer;
    transition:
      color var(--dur-fast) var(--ease),
      background-color var(--dur-fast) var(--ease);
  }
  .seg button.on {
    color: var(--accent-ink);
    background: var(--accent);
  }
  .flow__tag {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  /* ---- diagram (row) ---- */
  .diagram {
    display: flex;
    align-items: flex-start;
    padding-top: 4px;
  }
  .node {
    flex: 0 0 auto;
    width: clamp(6.2rem, 9vw, 8rem);
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 0.35rem;
    animation: rise var(--dur-slow) var(--ease) both;
    animation-delay: var(--d);
  }
  .node__ico {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: var(--r-md);
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-muted);
  }
  .node.private .node__ico {
    background: color-mix(in oklab, var(--accent) 16%, transparent);
    border-color: color-mix(in oklab, var(--accent) 45%, transparent);
    color: var(--accent);
  }
  .node__title {
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .node__sub {
    font-size: 0.72rem;
    line-height: 1.3;
    color: var(--text-faint);
  }

  .link {
    flex: 1 1 0;
    min-width: 46px;
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    /* nudge the track down to the vertical center of the node icons */
    padding-top: 26px;
    animation: fadein var(--dur-slow) var(--ease) both;
    animation-delay: var(--d);
  }
  .link__label {
    position: absolute;
    top: -2px;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-family: var(--font-mono);
    font-size: 0.62rem;
    letter-spacing: 0.02em;
    white-space: nowrap;
    color: var(--text-faint);
  }
  .link.private .link__label {
    color: var(--accent);
  }
  .track {
    position: relative;
    width: 100%;
    height: 2px;
    border-radius: 2px;
    background: var(--border-strong);
  }
  .link.private .track {
    background: color-mix(in oklab, var(--accent) 55%, transparent);
  }
  /* arrowhead */
  .track::after {
    content: '';
    position: absolute;
    right: -1px;
    top: 50%;
    width: 6px;
    height: 6px;
    border-top: 2px solid var(--border-strong);
    border-right: 2px solid var(--border-strong);
    transform: translateY(-50%) rotate(45deg);
  }
  .link.private .track::after {
    border-color: color-mix(in oklab, var(--accent) 70%, transparent);
  }
  .packet {
    position: absolute;
    top: 50%;
    left: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--brand-orange);
    box-shadow: 0 0 10px 1px var(--glow-orange);
    transform: translate(-50%, -50%);
    animation: flowX 1.9s linear infinite;
    animation-delay: var(--d);
  }
  .link.private .packet {
    background: var(--accent);
    box-shadow: 0 0 10px 1px var(--glow-teal);
  }

  .flow__summary {
    margin-top: 2.2rem;
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 70ch;
  }
  /* ---- autocrypt "on the wire" panel ---- */
  .wire {
    margin: 1.6rem 0 0;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg);
    overflow: hidden;
  }
  .wire figcaption {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.6rem 0.9rem;
    border-bottom: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .wire figcaption :global(svg) {
    color: var(--accent);
  }
  .wire__code {
    margin: 0;
    padding: 1rem 0.95rem;
    overflow-x: auto;
    font-family: var(--font-mono);
    font-size: 0.74rem;
    line-height: 1.75;
    color: var(--text-muted);
    white-space: pre;
  }
  .wire__code .dim {
    color: var(--text-faint);
  }
  .wire__code .ac {
    color: var(--accent);
    font-weight: 600;
  }
  .wire__code .pgp {
    color: var(--brand-orange);
  }
  .wire__code .cmt {
    color: var(--text-faint);
    font-style: italic;
  }
  .wire__note {
    margin-top: 1rem;
    color: var(--text-muted);
    font-size: var(--text-sm);
    max-width: 70ch;
  }
  .wire__note strong {
    color: var(--text);
    font-weight: 600;
  }

  .flow__legend {
    display: flex;
    flex-wrap: wrap;
    gap: 1.2rem;
    margin-top: 1.2rem;
    padding-top: 1.2rem;
    border-top: 1px solid var(--border);
  }
  .flow__legend li {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .dot.private {
    background: var(--accent);
    box-shadow: 0 0 8px var(--glow-teal);
  }
  .dot.public {
    background: var(--brand-orange);
    box-shadow: 0 0 8px var(--glow-orange);
  }

  @keyframes flowX {
    from {
      left: 4%;
      opacity: 0;
    }
    15% {
      opacity: 1;
    }
    85% {
      opacity: 1;
    }
    to {
      left: 96%;
      opacity: 0;
    }
  }
  @keyframes flowY {
    from {
      top: 6%;
      opacity: 0;
    }
    15% {
      opacity: 1;
    }
    85% {
      opacity: 1;
    }
    to {
      top: 94%;
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
  }
  @keyframes fadein {
    from {
      opacity: 0;
    }
  }

  /* ---- vertical layout on narrow screens ---- */
  @media (max-width: 720px) {
    .diagram {
      flex-direction: column;
      align-items: stretch;
      padding-top: 0;
    }
    .node {
      flex-direction: row;
      width: 100%;
      text-align: left;
      gap: 0.9rem;
      align-items: center;
    }
    .node__sub {
      font-size: var(--text-xs);
    }
    .link {
      flex: none;
      width: 52px;
      min-width: 0;
      height: 52px;
      padding-top: 0;
      align-items: flex-start;
    }
    /* draw the connector vertically, aligned under the icon centres (26px) */
    .track {
      position: absolute;
      left: 25px;
      top: 0;
      width: 2px;
      height: 100%;
    }
    .track::after {
      right: auto;
      top: auto;
      left: 50%;
      bottom: -1px;
      transform: translateX(-50%) rotate(135deg);
    }
    .packet {
      left: 25px;
      top: 0;
      animation-name: flowY;
    }
    .link__label {
      position: absolute;
      top: 50%;
      left: 46px;
      transform: translateY(-50%);
      font-size: 0.66rem;
      background: var(--surface-solid);
      padding: 2px 5px;
      border-radius: var(--r-sm);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .node,
    .link {
      animation: none;
    }
    .packet {
      animation: none;
      left: 50%;
      top: 50%;
      opacity: 1;
    }
  }
</style>
