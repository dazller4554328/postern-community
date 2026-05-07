<script lang="ts">
  import { onMount } from 'svelte';
  import {
    prefs,
    FONT_LABELS,
    FONT_STACKS,
    type Theme,
    type FontId,
    type DefaultView,
    type RowStyle
  } from '$lib/prefs';
  import {
    osNotificationPermission,
    requestOsPermission,
    type NotificationPermissionState
  } from '$lib/notifications';
  import InfoBubble from '$lib/components/InfoBubble.svelte';

  let theme = $state<Theme>('system');
  let font = $state<FontId>('system');
  let defaultView = $state<DefaultView>('html');
  let rowStyle = $state<RowStyle>('detailed');
  let notifyNewMail = $state(true);
  let notifySound = $state(true);
  let notifyOsToast = $state(false);
  let osPermission = $state<NotificationPermissionState>('unsupported');
  let sendUndoSecs = $state(10);
  let signatureOnReplies = $state(false);
  let composeGrammarCheck = $state(true);
  let zebraRows = $state(false);

  $effect(() => {
    const unsub = prefs.subscribe((p) => {
      theme = p.theme;
      font = p.font;
      defaultView = p.defaultView;
      rowStyle = p.rowStyle;
      notifyNewMail = p.notifyNewMail;
      notifySound = p.notifySound;
      notifyOsToast = p.notifyOsToast;
      sendUndoSecs = p.sendUndoSecs;
      signatureOnReplies = p.signatureOnReplies;
      composeGrammarCheck = p.composeGrammarCheck;
      zebraRows = p.zebraRows;
    });
    return unsub;
  });

  onMount(() => {
    osPermission = osNotificationPermission();
  });

  function setTheme(t: Theme) { prefs.update((p) => ({ ...p, theme: t })); }
  function setFont(f: FontId) { prefs.update((p) => ({ ...p, font: f })); }
  function setDefaultView(v: DefaultView) { prefs.update((p) => ({ ...p, defaultView: v })); }
  function setRowStyle(r: RowStyle) { prefs.update((p) => ({ ...p, rowStyle: r })); }
  function setNotifyNewMail(v: boolean) { prefs.update((p) => ({ ...p, notifyNewMail: v })); }
  function setNotifySound(v: boolean) { prefs.update((p) => ({ ...p, notifySound: v })); }
  function setSendUndoSecs(v: number) {
    const clamped = Math.round(Math.max(0, Math.min(60, v)));
    prefs.update((p) => ({ ...p, sendUndoSecs: clamped }));
  }
  function setSignatureOnReplies(v: boolean) {
    prefs.update((p) => ({ ...p, signatureOnReplies: v }));
  }
  function setComposeGrammarCheck(v: boolean) {
    prefs.update((p) => ({ ...p, composeGrammarCheck: v }));
  }
  function setZebraRows(v: boolean) {
    prefs.update((p) => ({ ...p, zebraRows: v }));
  }
  async function setNotifyOsToast(v: boolean) {
    // OS notifications need explicit permission before they'll fire. Ask on
    // enable so the toggle's on-state matches what the user will actually
    // see; if the user declines we flip back off so the UI doesn't lie.
    if (v && osPermission !== 'granted') {
      osPermission = await requestOsPermission();
      if (osPermission !== 'granted') {
        prefs.update((p) => ({ ...p, notifyOsToast: false }));
        return;
      }
    }
    prefs.update((p) => ({ ...p, notifyOsToast: v }));
  }

  const THEMES: { id: Theme; label: string; hint: string }[] = [
    { id: 'system', label: 'System', hint: 'Follow OS preference' },
    { id: 'light', label: 'Light', hint: '' },
    { id: 'dark', label: 'Dark', hint: '' },
    { id: 'cyberpunk', label: 'Cyberpunk', hint: 'Neon-noir' },
    { id: 'solarized-light', label: 'Solarized Light', hint: 'Schoonover warm light' },
    { id: 'solarized-dark', label: 'Solarized Dark', hint: 'Schoonover teal dark' },
    { id: 'dracula', label: 'Dracula', hint: 'Violet on slate' },
    { id: 'nord', label: 'Nord', hint: 'Arctic, quiet' },
    { id: 'gruvbox', label: 'Gruvbox', hint: 'Retro warm dark' },
    { id: 'monokai', label: 'Monokai', hint: 'Classic editor' },
    { id: 'sunset', label: 'Sunset', hint: 'Peach + ember light' },
    { id: 'forest', label: 'Forest', hint: 'Deep green, earthy' },
    { id: 'rose-pine', label: 'Rosé Pine', hint: 'Dusky rose dark' },
    { id: 'sepia', label: 'Sepia', hint: 'Aged-paper reading' },
    { id: 'acid-rain', label: 'Acid Rain', hint: 'Toxic neon storm' },
    { id: 'synth-candy', label: 'Synth Candy', hint: 'Bubblegum pop light' },
    { id: 'volcanic', label: 'Volcanic', hint: 'Lava glass dark' },
    { id: 'abyssal', label: 'Abyssal', hint: 'Deep sea glow' },
    { id: 'arcade', label: 'Arcade', hint: 'Electric cabinet dark' }
  ];
  const FONTS: FontId[] = ['system', 'serif', 'rounded', 'mono'];
  const VIEWS: { id: DefaultView; label: string; hint: string }[] = [
    { id: 'html', label: 'HTML', hint: 'Sanitized HTML in a sandboxed iframe (tracker pixels blocked)' },
    { id: 'plain', label: 'Plain', hint: 'Mailpile-style — extract and show text only, no HTML at all' },
    { id: 'source', label: 'Source', hint: 'Raw RFC822 source, for the paranoid' }
  ];
  const ROW_STYLES: { id: RowStyle; label: string; hint: string }[] = [
    { id: 'detailed', label: 'Detailed', hint: 'Sender, subject, and the first part of the message inline. No hover needed.' },
    { id: 'compact', label: 'Compact', hint: 'Sender, subject, and "2h ago". Hover a row to peek at the snippet (Mailpile-style).' }
  ];
</script>

<section class="panel">
  <div class="section-head">
    <h2>Appearance &amp; reading</h2>
    <p>These apply instantly — one click, no save needed.</p>
  </div>

  <div class="field">
    <div class="field-label">
      <label>Theme</label>
    </div>
    <div class="pill-choice">
      {#each THEMES as t (t.id)}
        <button class:active={theme === t.id} onclick={() => setTheme(t.id)}>{t.label}</button>
      {/each}
    </div>
  </div>

  <div class="field">
    <div class="field-label">
      <label>Font</label>
      <span class="field-sub">System stacks — nothing downloaded.</span>
    </div>
    <div class="font-grid">
      {#each FONTS as f (f)}
        <button
          class="font-card"
          class:active={font === f}
          style="font-family: {FONT_STACKS[f]}"
          onclick={() => setFont(f)}
        >
          <span class="font-name">{FONT_LABELS[f]}</span>
          <span class="font-preview">The quick brown fox jumps over the lazy dog. 0123</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <div class="field-label">
      <label>Default view for messages</label>
      <span class="field-sub">Can be switched per-message from the toolbar above each mail.</span>
    </div>
    <div class="view-choice">
      {#each VIEWS as v (v.id)}
        <button class:active={defaultView === v.id} onclick={() => setDefaultView(v.id)}>
          <span class="view-label">{v.label}</span>
          <span class="view-hint">{v.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <div class="field-label">
      <label>Message list layout</label>
    </div>
    <div class="view-choice">
      {#each ROW_STYLES as r (r.id)}
        <button class:active={rowStyle === r.id} onclick={() => setRowStyle(r.id)}>
          <span class="view-label">{r.label}</span>
          <span class="view-hint">{r.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Zebra striping
        <InfoBubble text="Tints every second row in the message list. Easier on the eyes when scanning long lists; off keeps the default subtle alternation." />
      </strong>
      <span class="field-sub">
        {zebraRows ? 'On — every second row gets a tinted background.' : 'Off — default subtle alternation.'}
      </span>
    </div>
    <label class="switch" title={zebraRows ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={zebraRows} onchange={(e) => setZebraRows((e.currentTarget as HTMLInputElement).checked)} />
      <span class="track"></span>
    </label>
  </div>

  <div class="section-head" style="margin-top: 1.25rem;">
    <h2>New-mail notifications</h2>
    <p>Signals that fire while Postern is open. Phase 2 (push to locked phones) is on the roadmap.</p>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Notify me on new mail
        <InfoBubble text="When enabled, an in-app toast appears and the tab title flashes whenever new inbox mail arrives. The underlying signal is a poll of the server's folder counts every 30 seconds while this tab is visible." />
      </strong>
      <span class="field-sub">
        {notifyNewMail ? 'On — toast + title flash on arrival.' : 'Off.'}
      </span>
    </div>
    <label class="switch" title={notifyNewMail ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={notifyNewMail} onchange={(e) => setNotifyNewMail((e.currentTarget as HTMLInputElement).checked)} />
      <span class="track"></span>
    </label>
  </div>

  {#if notifyNewMail}
    <div class="row">
      <div class="label">
        <strong class="inline">
          Play sound
          <InfoBubble text="A short two-note chime generated in the browser — no audio file downloaded. Browsers require one click or keypress in the tab before audio can play, so the first arrival after a fresh page load may be silent." />
        </strong>
        <span class="field-sub">
          {notifySound ? 'On — soft chime on arrival.' : 'Silent.'}
        </span>
      </div>
      <label class="switch" title={notifySound ? 'Turn off' : 'Turn on'}>
        <input type="checkbox" checked={notifySound} onchange={(e) => setNotifySound((e.currentTarget as HTMLInputElement).checked)} />
        <span class="track"></span>
      </label>
    </div>

    <div class="row">
      <div class="label">
        <strong class="inline">
          System notifications
          <InfoBubble text="Surface a native OS banner (macOS Notification Center, Windows toast, Android system tray) in addition to the in-app toast. Requires browser permission the first time. On locked Android phones these only fire while the Postern tab is open in the foreground — for true background push you'll want Phase 2." />
        </strong>
        <span class="field-sub">
          {#if osPermission === 'unsupported'}
            Not supported by this browser.
          {:else if osPermission === 'denied'}
            Permission denied — re-enable in your browser's site settings first.
          {:else if notifyOsToast}
            On — OS banner will fire alongside the in-app toast.
          {:else}
            Off.
          {/if}
        </span>
      </div>
      <label class="switch" title={notifyOsToast ? 'Turn off' : 'Turn on'}>
        <input
          type="checkbox"
          checked={notifyOsToast}
          disabled={osPermission === 'unsupported' || osPermission === 'denied'}
          onchange={(e) => setNotifyOsToast((e.currentTarget as HTMLInputElement).checked)}
        />
        <span class="track"></span>
      </label>
    </div>
  {/if}

  <div class="section-head" style="margin-top: 1.25rem;">
    <h2>Compose &amp; send</h2>
    <p>Undo delay and signature defaults for outgoing mail.</p>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Undo send window
        <InfoBubble text="Seconds to hold a new message in the outbox before it's actually handed to SMTP. While the window is open, hitting Undo in compose restores the draft verbatim. Set to 0 to bypass the window; the server still queues the send but the worker picks it up on its next tick (≤ 2s)." />
      </strong>
      <span class="field-sub">
        {sendUndoSecs === 0
          ? 'Off — sends dispatch on the next worker tick.'
          : `On — ${sendUndoSecs} second${sendUndoSecs === 1 ? '' : 's'} to cancel after clicking Send.`}
      </span>
    </div>
    <select
      value={sendUndoSecs}
      onchange={(e) => setSendUndoSecs(Number((e.currentTarget as HTMLSelectElement).value))}
    >
      <option value={0}>Off</option>
      <option value={5}>5 seconds</option>
      <option value={10}>10 seconds</option>
      <option value={20}>20 seconds</option>
      <option value={30}>30 seconds</option>
      <option value={60}>60 seconds</option>
    </select>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Auto-insert signature on replies
        <InfoBubble text="When on, the account signature is auto-inserted above the quoted thread when composing a reply or forward. Off by default — most conventions keep signatures only on the initial outbound message, not every reply." />
      </strong>
      <span class="field-sub">
        {signatureOnReplies
          ? 'On — signature appears above the quote block.'
          : 'Off — signatures only auto-insert on new messages.'}
      </span>
    </div>
    <label class="switch" title={signatureOnReplies ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={signatureOnReplies} onchange={(e) => setSignatureOnReplies((e.currentTarget as HTMLInputElement).checked)} />
      <span class="track"></span>
    </label>
  </div>

  <div class="row">
    <div class="label">
      <strong class="inline">
        Grammar &amp; spelling suggestions
        <InfoBubble text="Runs the local Harper grammar / spell checker against the compose body and surfaces suggestions you can apply with one click. Disable if you find the suggestions noisy or just want a faster, lighter compose pane — Harper's wasm bundle stops loading at all when this is off." />
      </strong>
      <span class="field-sub">
        {composeGrammarCheck
          ? 'On — Harper checks the compose body and shows fixable suggestions.'
          : 'Off — no grammar panel, wasm dictionary skipped.'}
      </span>
    </div>
    <label class="switch" title={composeGrammarCheck ? 'Turn off' : 'Turn on'}>
      <input type="checkbox" checked={composeGrammarCheck} onchange={(e) => setComposeGrammarCheck((e.currentTarget as HTMLInputElement).checked)} />
      <span class="track"></span>
    </label>
  </div>
</section>
