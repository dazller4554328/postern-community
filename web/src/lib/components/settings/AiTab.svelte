<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type AiCommandmentsResponse,
    type AiCoverage,
    type AiEmbedProviderKind,
    type AiProviderKind,
    type AiSettingsDto,
    type AiSettingsTestResult,
    type AiStatus,
    type PrivacyPosture
  } from '$lib/api';
  import InfoBubble from '$lib/components/InfoBubble.svelte';
  import AiActivityTab from '$lib/components/settings/AiActivityTab.svelte';
  import AiHistoryTab from '$lib/components/settings/AiHistoryTab.svelte';
  import { lockdown, refreshLockdown, setLockdown } from '$lib/lockdown';

  let lockdownBusy = $state(false);

  // Re-index state. `reindexResult` shows the deletion count for
  // a beat after success, then refresh() clears it implicitly via
  // a state reset.
  let reindexBusy = $state(false);
  let reindexResult = $state<number | null>(null);
  let reindexError = $state<string | null>(null);

  async function confirmAndReindex() {
    if (reindexBusy) return;
    const confirmed = confirm(
      'Clear all existing embeddings and re-index from scratch?\n\n' +
        'This wipes the stored vectors. The indexer will rebuild on its next ' +
        'tick (within ~60 seconds) and takes ~1–2 hours on local Ollama for a ' +
        'large mailbox. Datas Q&A returns no results until the rebuild reaches ' +
        'the relevant emails. Cost: zero on Ollama, otherwise depends on the ' +
        'configured embed provider.\n\n' +
        'You usually want this after the embedding format changes (e.g. when ' +
        'sender headers were added) so old + new vectors don\'t produce ' +
        'inconsistent retrieval.'
    );
    if (!confirmed) return;
    reindexBusy = true;
    reindexResult = null;
    reindexError = null;
    try {
      const r = await api.aiClearEmbeddings();
      reindexResult = r.deleted;
      // Refresh coverage so the progress bar resets to 0 / total.
      try {
        coverage = await api.aiCoverage();
      } catch {
        /* non-fatal */
      }
    } catch (e) {
      reindexError = e instanceof Error ? e.message : String(e);
    } finally {
      reindexBusy = false;
    }
  }
  let lockdownError = $state<string | null>(null);

  async function toggleLockdown() {
    if (lockdownBusy) return;
    lockdownBusy = true;
    lockdownError = null;
    try {
      await setLockdown(!$lockdown.enabled);
    } catch (e) {
      lockdownError = e instanceof Error ? e.message : String(e);
    } finally {
      lockdownBusy = false;
    }
  }

  /// What the user is editing on this page. Diverges from `loaded`
  /// once they touch any field; saved only on submit.
  interface FormState {
    enabled: boolean;
    provider_kind: AiProviderKind;
    chat_model: string;
    embed_model: string;
    base_url: string;
    /// User typed a new chat key this session. "" = clear,
    /// null = "leave the existing key in place".
    api_key_input: string | null;
    cloud_consent: boolean;
    /// Embed-side fields. Default ollama → bulk content stays local.
    embed_provider_kind: AiEmbedProviderKind;
    embed_base_url: string;
    /// Same three-state semantics as api_key_input.
    embed_api_key_input: string | null;
    /// "Always on" — auto-rehydrate providers after vault unlock.
    auto_start: boolean;
    /// User-defined extension to the prompt (additional rules
    /// appended after the seven Commandments). Empty string =
    /// no extension.
    user_rules: string;
    /// Newline-delimited sender exclusion patterns. `*` is a
    /// wildcard, `#` starts a comment line.
    excluded_senders: string;
    /// Newline-delimited label exclusion list (exact match).
    excluded_labels: string;
    /// Optional model override used by the compose-pane Polish
    /// rewrite. Empty string = inherit chat_model.
    polish_chat_model: string;
    /// Datas response-freedom mode. Empty string = balanced (default).
    freedom_mode: 'strict' | 'balanced' | 'open' | '';
    /// Per-request output-token cap. 0 = "use default".
    chat_max_tokens: number;
  }

  let loaded = $state<AiSettingsDto | null>(null);
  let status = $state<AiStatus | null>(null);
  let coverage = $state<AiCoverage | null>(null);
  let form = $state<FormState>({
    enabled: true,
    provider_kind: 'ollama',
    chat_model: '',
    embed_model: '',
    base_url: '',
    api_key_input: null,
    cloud_consent: false,
    embed_provider_kind: 'ollama',
    embed_base_url: '',
    embed_api_key_input: null,
    auto_start: true,
    user_rules: '',
    excluded_senders: '',
    excluded_labels: '',
    polish_chat_model: '',
    freedom_mode: 'balanced',
    chat_max_tokens: 0
  });

  /// Commandments + rendered prompt, fetched on mount. Read-only
  /// list of the seven non-negotiable rules — surfaced so the
  /// user can see what Datas is told before it sees their mail.
  let commandments = $state<AiCommandmentsResponse | null>(null);
  let commandmentsExpanded = $state(false);
  let promptPreviewOpen = $state(false);

  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveOk = $state(false);
  let testResult = $state<AiSettingsTestResult | null>(null);
  let testing = $state(false);
  let clearing = $state(false);
  let loading = $state(true);

  /// Models the active chat provider currently has installed —
  /// drives the Polish-model dropdown. Independent of `loaded` so
  /// transient provider unreachability doesn't blank the form.
  let modelList = $state<string[]>([]);
  let modelListLoading = $state(false);
  let modelListError = $state<string | null>(null);
  let modelListProvider = $state<string>('');

  /// Chat-model field mode: 'pick' (dropdown of installed models)
  /// or 'custom' (free-text input). Mode is local UI state — the
  /// saved value is the same `form.chat_model` either way. Default
  /// is 'pick' so the user gets the dropdown; the ✎ button flips to
  /// free-text for backends whose list endpoint is unreliable or
  /// when the model the user wants isn't on the catalogue yet.
  let chatModelMode = $state<'pick' | 'custom'>('pick');

  async function refreshModels() {
    modelListLoading = true;
    modelListError = null;
    try {
      const r = await api.aiListModels();
      modelList = r.models;
      modelListProvider = r.provider;
      modelListError = r.error;
    } catch (e) {
      modelListError = e instanceof Error ? e.message : String(e);
      modelList = [];
    } finally {
      modelListLoading = false;
    }
  }

  /// Provider kind drives the visibility of the API-key + base URL
  /// rows. Switching kinds resets the input fields so users don't
  /// accidentally save (say) an OpenAI key against an Anthropic
  /// selection.
  let isCloudKind = $derived(
    form.provider_kind === 'anthropic' ||
      form.provider_kind === 'openai' ||
      (form.provider_kind === 'openai_compat' && isHostedCompat(form.base_url))
  );
  let needsApiKey = $derived(form.provider_kind !== 'ollama');
  let needsBaseUrl = $derived(form.provider_kind === 'openai_compat');
  let supportsBaseUrlOverride = $derived(form.provider_kind === 'ollama');

  // Embed-side derived flags — same shape as chat, narrowed because
  // anthropic isn't a valid embed kind (no embeddings API).
  let isEmbedCloudKind = $derived(
    form.embed_provider_kind === 'openai' ||
      (form.embed_provider_kind === 'openai_compat' && isHostedCompat(form.embed_base_url))
  );
  let embedNeedsBaseUrl = $derived(form.embed_provider_kind === 'openai_compat');
  let embedSupportsBaseUrlOverride = $derived(form.embed_provider_kind === 'ollama');
  /// True when the embed provider differs from chat AND requires a
  /// key — that's the only case where we need a separate "Embed
  /// API key" input. When chat == embed (e.g. both OpenAI), the
  /// chat key is reused; when embed = Ollama, no key is needed.
  let embedNeedsOwnKey = $derived(
    isEmbedCloudKind && form.embed_provider_kind !== form.provider_kind
  );
  let cloudConsentRequired = $derived(isCloudKind || isEmbedCloudKind);

  function isHostedCompat(url: string): boolean {
    const lower = url.toLowerCase();
    return [
      'api.x.ai',
      'api.groq.com',
      'api.together.xyz',
      'api.perplexity.ai',
      'api.deepseek.com',
      'api.mistral.ai'
    ].some((m) => lower.includes(m));
  }

  async function refresh() {
    loading = true;
    try {
      const [settings, st, cov] = await Promise.all([
        api.aiGetSettings(),
        api.aiStatus(),
        api.aiCoverage().catch(() => null)
      ]);
      loaded = settings;
      status = st;
      coverage = cov;
      form = {
        enabled: settings.enabled,
        provider_kind: settings.provider_kind,
        chat_model: settings.chat_model,
        embed_model: settings.embed_model,
        base_url: settings.base_url ?? '',
        api_key_input: null,
        cloud_consent: settings.cloud_consent,
        embed_provider_kind: settings.embed_provider_kind,
        embed_base_url: settings.embed_base_url ?? '',
        embed_api_key_input: null,
        auto_start: settings.auto_start,
        user_rules: settings.user_rules ?? '',
        excluded_senders: settings.excluded_senders ?? '',
        excluded_labels: settings.excluded_labels ?? '',
        polish_chat_model: settings.polish_chat_model ?? '',
        freedom_mode:
          settings.freedom_mode === 'strict' ||
          settings.freedom_mode === 'open'
            ? settings.freedom_mode
            : 'balanced',
        chat_max_tokens: settings.chat_max_tokens ?? 0
      };
      // Best-effort fetch of installed models so the Polish-model
      // dropdown below is populated. Independent of the main load —
      // a provider that doesn't expose a model list (or is offline
      // right now) shouldn't block the rest of the panel from
      // rendering.
      void refreshModels();
      // Fetch the Commandments + rendered prompt alongside the
      // settings load so the panel is fully populated on first
      // paint. Best-effort — if it fails, we just don't render
      // the section.
      try {
        commandments = await api.aiCommandments();
      } catch (e) {
        console.warn('aiCommandments failed', e);
      }
    } catch (e) {
      console.error('ai settings load failed', e);
    } finally {
      loading = false;
    }
  }

  function onKindChange(next: AiProviderKind) {
    if (next === form.provider_kind) return;
    form = {
      ...form,
      provider_kind: next,
      // Clear the typed key + base URL when switching kinds — keeping
      // them risks saving the wrong credential against the wrong
      // backend. Persisted key (in `loaded.api_key_set`) is left alone
      // until the user submits.
      api_key_input: null,
      base_url: next === 'openai_compat' ? form.base_url : '',
      // Re-arm consent so the user re-confirms when picking a cloud
      // provider; harmless to leave true otherwise.
      cloud_consent: next === 'ollama' && form.embed_provider_kind === 'ollama'
        ? false
        : form.cloud_consent
    };
    testResult = null;
    saveOk = false;
  }

  function onEmbedKindChange(next: AiEmbedProviderKind) {
    if (next === form.embed_provider_kind) return;
    form = {
      ...form,
      embed_provider_kind: next,
      embed_api_key_input: null,
      embed_base_url: next === 'openai_compat' ? form.embed_base_url : '',
      cloud_consent:
        next === 'ollama' && form.provider_kind === 'ollama'
          ? false
          : form.cloud_consent
    };
    saveOk = false;
  }

  async function runTest() {
    testing = true;
    testResult = null;
    saveError = null;
    try {
      testResult = await api.aiTestSettings({
        provider_kind: form.provider_kind,
        chat_model: form.chat_model || undefined,
        embed_model: form.embed_model || undefined,
        base_url: form.base_url || null,
        api_key: form.api_key_input
      });
    } catch (e) {
      testResult = {
        ok: false,
        provider: form.provider_kind,
        privacy_posture: 'unknown',
        message: e instanceof Error ? e.message : String(e)
      };
    } finally {
      testing = false;
    }
  }

  async function save() {
    saveError = null;
    saveOk = false;
    if (cloudConsentRequired && !form.cloud_consent) {
      saveError =
        'Tick the cloud-consent confirmation — this configuration sends mail content off your box.';
      return;
    }
    saving = true;
    try {
      const next = await api.aiUpdateSettings({
        enabled: form.enabled,
        provider_kind: form.provider_kind,
        chat_model: form.chat_model || undefined,
        embed_model: form.embed_model || undefined,
        base_url: form.base_url || null,
        api_key: form.api_key_input,
        embed_provider_kind: form.embed_provider_kind,
        embed_base_url: form.embed_base_url || null,
        embed_api_key: form.embed_api_key_input,
        cloud_consent: form.cloud_consent,
        auto_start: form.auto_start,
        // Trim whitespace-only inputs to "" so the server clears
        // the column rather than storing a meaningless space.
        user_rules: form.user_rules.trim(),
        excluded_senders: form.excluded_senders.trim(),
        excluded_labels: form.excluded_labels.trim(),
        polish_chat_model: form.polish_chat_model.trim(),
        freedom_mode: form.freedom_mode || 'balanced',
        chat_max_tokens: form.chat_max_tokens
      });
      loaded = next;
      // Refresh status (the holder just hot-swapped) so the panel
      // reflects the live posture, not the form's claim.
      status = await api.aiStatus();
      // Re-fetch the rendered prompt so the live preview reflects
      // the user_rules just saved.
      try {
        commandments = await api.aiCommandments();
      } catch {
        /* non-fatal */
      }
      form = {
        ...form,
        // Don't carry the typed keys into subsequent saves — once
        // they're on file `*_key_set` will be true and the
        // placeholder UI takes over.
        api_key_input: null,
        embed_api_key_input: null,
        cloud_consent: next.cloud_consent,
        user_rules: next.user_rules ?? '',
        excluded_senders: next.excluded_senders ?? '',
        excluded_labels: next.excluded_labels ?? '',
        polish_chat_model: next.polish_chat_model ?? '',
        freedom_mode:
          next.freedom_mode === 'strict' || next.freedom_mode === 'open'
            ? next.freedom_mode
            : 'balanced',
        chat_max_tokens: next.chat_max_tokens ?? 0
      };
      // Provider/model may have just changed — re-fetch the installed
      // model list so the Polish dropdown reflects the new backend.
      void refreshModels();
      // Refresh the coverage display so the count reflects the
      // auto-prune that ran server-side as part of save.
      try {
        coverage = await api.aiCoverage();
      } catch {
        /* non-fatal */
      }
      saveOk = true;
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  async function clearHistory() {
    if (!coverage || coverage.chat_history_count === 0) return;
    const ok = confirm(
      `Forget all ${coverage.chat_history_count} stored conversations? This cannot be undone.`
    );
    if (!ok) return;
    clearing = true;
    try {
      await api.aiClearHistory();
      coverage = await api.aiCoverage().catch(() => null);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      clearing = false;
    }
  }

  function postureLabel(p: PrivacyPosture | null | undefined): string {
    if (!p) return 'unknown';
    if (p === 'local_only') return 'Local only';
    if (p === 'user_controlled_remote') return 'Your remote box';
    return 'Third-party cloud';
  }
  function postureClass(p: PrivacyPosture | null | undefined): string {
    if (!p) return '';
    if (p === 'local_only') return 'posture-local';
    if (p === 'user_controlled_remote') return 'posture-self';
    return 'posture-cloud';
  }

  function indexedPercent(c: AiCoverage | null): number {
    if (!c || c.total === 0) return 0;
    return Math.min(100, Math.round((c.indexed / c.total) * 100));
  }

  function providerLabel(k: AiProviderKind): string {
    switch (k) {
      case 'ollama':
        return 'Ollama (local)';
      case 'anthropic':
        return 'Anthropic Claude';
      case 'openai':
        return 'OpenAI';
      case 'openai_compat':
        return 'OpenAI-compatible (Grok, vLLM, …)';
    }
  }

  function chatPlaceholder(k: AiProviderKind): string {
    if (k === 'anthropic') return 'claude-sonnet-4-6';
    if (k === 'openai') return 'gpt-4o-mini';
    if (k === 'openai_compat') return 'grok-beta';
    return 'llama3.1:8b-instruct-q4_K_M';
  }
  function embedPlaceholder(k: AiProviderKind): string {
    if (k === 'openai') return 'text-embedding-3-small';
    return 'nomic-embed-text';
  }

  onMount(() => {
    void refresh();
    void refreshLockdown();
  });

  // Sub-tabs: Status & Setup (current panel) / Activity (per-call
  // log + cost) / History (the Q&A round-trip log).
  type SubTab = 'setup' | 'activity' | 'history';
  let subTab = $state<SubTab>('setup');
</script>

<section class="panel">
  <div class="section-head">
    <h2>AI</h2>
    <p>
      Pick a backend for Datas + semantic search. Local Ollama keeps
      every byte on your box; cloud providers (Claude, OpenAI, Grok) need an
      API key and send retrieved email excerpts off-box on each query.
      Embeddings stay local unless you specifically pick OpenAI for chat —
      keeping the most-touched data path private even when chat is hosted.
    </p>
  </div>

  <nav class="sub-tabs" role="tablist" aria-label="AI sub-sections">
    <button
      role="tab"
      aria-selected={subTab === 'setup'}
      class:active={subTab === 'setup'}
      onclick={() => (subTab = 'setup')}
    >Status &amp; Setup</button>
    <button
      role="tab"
      aria-selected={subTab === 'activity'}
      class:active={subTab === 'activity'}
      onclick={() => (subTab = 'activity')}
    >Activity</button>
    <button
      role="tab"
      aria-selected={subTab === 'history'}
      class:active={subTab === 'history'}
      onclick={() => (subTab = 'history')}
    >History</button>
  </nav>

  {#if subTab === 'activity'}
    <AiActivityTab />
  {:else if subTab === 'history'}
    <AiHistoryTab />
  {:else if loading}
    <p class="muted">Loading…</p>
  {:else if !loaded}
    <p class="muted">Could not load AI settings.</p>
  {:else}
    <!-- ─────────── Master enable ─────────── -->
    <div class="row">
      <div class="label">
        <strong class="inline">
          AI features
          <InfoBubble text="Master switch. When off, the chat + embed providers are unloaded, the Datas box hides, and the indexer stops without affecting any already-indexed messages." />
        </strong>
        <span class="field-sub">
          {#if status?.enabled}
            Live — see backends below.
          {:else if form.enabled}
            Configured but not currently reachable. Try Test connection.
          {:else}
            Disabled.
          {/if}
        </span>
      </div>
      <div class="enable-toggle">
        <label class="switch">
          <input type="checkbox" bind:checked={form.enabled} />
          <span class="slider" aria-hidden="true"></span>
          <span class="switch-label">{form.enabled ? 'On' : 'Off'}</span>
        </label>
      </div>
    </div>

    <!-- ─────────── Live backend status (chat + embed, parallel) ─────────── -->
    {#if status?.enabled}
      <div class="backend-rows">
        <div class="backend-row">
          <span class="backend-label">Chat backend</span>
          <code class="backend-name">{status.provider ?? '—'}</code>
          {#if status.privacy_posture}
            <span class="posture {postureClass(status.privacy_posture)}">
              {postureLabel(status.privacy_posture)}
            </span>
          {/if}
        </div>
        <div class="backend-row">
          <span class="backend-label">Embed backend</span>
          {#if status.embed_provider}
            <code class="backend-name">{status.embed_provider}</code>
            {#if status.embed_privacy_posture}
              <span class="posture {postureClass(status.embed_privacy_posture)}">
                {postureLabel(status.embed_privacy_posture)}
              </span>
            {/if}
          {:else}
            <span class="backend-missing">not loaded — check Embed base URL + the host's reachability</span>
          {/if}
        </div>
      </div>
    {/if}

    <div class="ai-form-grid">
      <div class="ai-form-card">
        <h3>Chat</h3>
        <!-- ─────────── Chat provider selection ─────────── -->
        <div class="field">
          <div class="field-label">
            <label for="ai-provider-kind">Provider</label>
            <InfoBubble text="Where Datas's answers come from. Local Ollama is private + free but slower / lower-quality. Anthropic + OpenAI / xAI need an API key from their dashboard." />
          </div>
          <select
            id="ai-provider-kind"
            bind:value={form.provider_kind}
            onchange={(e) => onKindChange((e.currentTarget as HTMLSelectElement).value as AiProviderKind)}
          >
            <option value="ollama">{providerLabel('ollama')}</option>
            <option value="anthropic">{providerLabel('anthropic')}</option>
            <option value="openai">{providerLabel('openai')}</option>
            <option value="openai_compat">{providerLabel('openai_compat')}</option>
          </select>
        </div>

        <!-- ─────────── Base URL (Ollama override / openai_compat) ─────────── -->
        {#if needsBaseUrl || supportsBaseUrlOverride}
      <div class="field">
        <div class="field-label">
          <label for="ai-base-url">Base URL</label>
          <InfoBubble text={needsBaseUrl
            ? 'Required. e.g. https://api.x.ai/v1 for Grok, or http://vllm.lan:8000/v1 for self-hosted.'
            : 'Override where Postern looks for Ollama. Leave blank for the default localhost:11434.'} />
        </div>
        <input
          id="ai-base-url"
          type="url"
          bind:value={form.base_url}
          placeholder={needsBaseUrl ? 'https://api.x.ai/v1' : 'http://localhost:11434'}
          autocomplete="off"
          spellcheck="false"
        />
      </div>
        {/if}

        <!-- ─────────── API key (cloud + compat) ─────────── -->
        {#if needsApiKey}
      <div class="field">
        <div class="field-label">
          <label for="ai-api-key">API key</label>
          <InfoBubble text="Stored encrypted in the vault, never returned over the network. Leave the field empty to keep the existing key. Type a new value to rotate. Type and submit blank-with-the-clear-button to remove." />
        </div>
        <div class="key-row">
          <input
            id="ai-api-key"
            type="password"
            placeholder={loaded.api_key_set ? '•••• key on file — leave blank to keep' : 'paste your API key'}
            value={form.api_key_input ?? ''}
            oninput={(e) => (form.api_key_input = (e.currentTarget as HTMLInputElement).value)}
            autocomplete="off"
            spellcheck="false"
          />
          {#if loaded.api_key_set}
            <button
              type="button"
              class="btn ghost"
              onclick={() => (form.api_key_input = '')}
              title="Submit empty to remove the stored key"
            >Clear</button>
          {/if}
        </div>
        <p class="field-sub">
          {#if form.api_key_input === null}
            {loaded.api_key_set ? 'Existing key will be reused.' : 'No key on file.'}
          {:else if form.api_key_input === ''}
            Submitting will <strong>remove</strong> the stored key.
          {:else}
            Submitting will <strong>replace</strong> the stored key.
          {/if}
        </p>
      </div>
        {/if}

        <!-- ─────────── Chat model ─────────── -->
        <div class="field">
          <div class="field-label">
            <label for="ai-chat-model">Model</label>
            <InfoBubble text="The chat model used for Ask Datas (RAG over your inbox). Pick from what your provider has installed/exposed; switch to 'Custom…' to type a model id manually for backends whose list endpoint is unreliable." />
          </div>
          <div class="polish-row">
            {#if chatModelMode === 'pick'}
              <select
                id="ai-chat-model"
                bind:value={form.chat_model}
                disabled={modelListLoading}
              >
                <option value="">(default for {providerLabel(form.provider_kind)} — {chatPlaceholder(form.provider_kind)})</option>
                {#if form.chat_model && !modelList.includes(form.chat_model)}
                  <option value={form.chat_model}>{form.chat_model} (saved, not in current list)</option>
                {/if}
                {#each modelList as m (m)}
                  <option value={m}>{m}</option>
                {/each}
              </select>
            {:else}
              <input
                id="ai-chat-model"
                type="text"
                bind:value={form.chat_model}
                placeholder={chatPlaceholder(form.provider_kind)}
                autocomplete="off"
                spellcheck="false"
              />
            {/if}
            <button
              type="button"
              class="btn ghost small"
              onclick={() => (chatModelMode = chatModelMode === 'pick' ? 'custom' : 'pick')}
              title={chatModelMode === 'pick' ? 'Switch to free-text input' : 'Switch back to dropdown'}
            >
              {chatModelMode === 'pick' ? '✎' : '☰'}
            </button>
          </div>
        </div>

        <!-- ─────────── Polish model (compose-pane rewrite) ─────────── -->
        <div class="field">
          <div class="field-label">
            <label for="ai-polish-model">Polish model</label>
            <InfoBubble text="Used by the compose-pane 'Polish selection' button. Defaults to the same model as Ask Datas above — pick a smaller/cheaper one (e.g. gpt-4o-mini, llama3.2:3b) here if you want polish to be quick and Ask to stay heavy. Leave blank to inherit." />
          </div>
          <div class="polish-row">
            <select
              id="ai-polish-model"
              bind:value={form.polish_chat_model}
              disabled={modelListLoading}
            >
              <option value="">(inherit chat model{form.chat_model ? ` — ${form.chat_model}` : ''})</option>
              {#if form.polish_chat_model && !modelList.includes(form.polish_chat_model)}
                <option value={form.polish_chat_model}>{form.polish_chat_model} (saved, not in current list)</option>
              {/if}
              {#each modelList as m (m)}
                <option value={m}>{m}</option>
              {/each}
            </select>
            <button
              type="button"
              class="btn ghost small"
              onclick={refreshModels}
              disabled={modelListLoading}
              title="Re-fetch installed models from the active provider"
            >
              {modelListLoading ? '…' : '↻'}
            </button>
          </div>
          {#if modelListError}
            <p class="field-sub warn">⚠ {modelListError}</p>
          {:else if modelList.length === 0 && !modelListLoading}
            <p class="field-sub">
              No models reported by {modelListProvider || 'the active provider'} yet —
              save Settings (or click ↻) once the provider is reachable to populate this list.
            </p>
          {:else if modelListProvider}
            <p class="field-sub">
              {modelList.length} model{modelList.length === 1 ? '' : 's'} from {modelListProvider}.
            </p>
          {/if}
        </div>
      </div>

      <div class="ai-form-card">
        <h3>Embeddings</h3>
        <!-- ─────────── Embedding provider selection ─────────── -->
        <div class="field">
          <div class="field-label">
            <label for="ai-embed-provider-kind">Provider</label>
            <InfoBubble text="Where the indexer + retrieval embeddings run. This is independent of the chat provider — most users want chat=cloud (best answers) + embed=Ollama (local, free, every email body stays on your box during indexing). Ollama is recommended unless you specifically want OpenAI's embeddings." />
          </div>
          <select
            id="ai-embed-provider-kind"
            bind:value={form.embed_provider_kind}
            onchange={(e) => onEmbedKindChange((e.currentTarget as HTMLSelectElement).value as AiEmbedProviderKind)}
          >
            <option value="ollama">{providerLabel('ollama')} — recommended (local, free, private)</option>
            <option value="openai">{providerLabel('openai')} — paid, sends every email body</option>
            <option value="openai_compat">{providerLabel('openai_compat')}</option>
          </select>
          {#if form.embed_provider_kind === 'ollama' && form.provider_kind !== 'ollama'}
            <p class="field-sub good">
              ✓ Embeddings stay local. Only the 6 emails retrieved per question
              are sent to {providerLabel(form.provider_kind)}.
            </p>
          {:else if form.embed_provider_kind === 'openai'}
            <p class="field-sub warn">
              ⚠ Every email body (~2 KB each) will be sent to OpenAI during
              indexing. ~$0.50 one-time for 50k messages.
            </p>
          {/if}
        </div>

        <!-- Base URL when embed_provider_kind is openai_compat or ollama-with-override. -->
        {#if embedNeedsBaseUrl || (embedSupportsBaseUrlOverride && form.provider_kind !== 'ollama')}
      <div class="field">
        <div class="field-label">
          <label for="ai-embed-base-url">Embed base URL</label>
          <InfoBubble text={embedNeedsBaseUrl
            ? 'Required for OpenAI-compatible embed providers. e.g. http://vllm.lan:8000/v1.'
            : 'Override where the embed Ollama lives. Leave blank to use localhost:11434 — same as the chat Ollama if both are local.'} />
        </div>
        <input
          id="ai-embed-base-url"
          type="url"
          bind:value={form.embed_base_url}
          placeholder={embedNeedsBaseUrl ? 'http://vllm.lan:8000/v1' : 'http://localhost:11434'}
          autocomplete="off"
          spellcheck="false"
        />
      </div>
        {/if}

        <!-- Embed-side API key — only when embed is cloud AND distinct from chat. -->
        {#if embedNeedsOwnKey}
      <div class="field">
        <div class="field-label">
          <label for="ai-embed-api-key">Embed API key</label>
          <InfoBubble text="Required because the embed provider differs from the chat provider — the chat key won't authorize on a different vendor's API. Same encryption + storage as the chat key." />
        </div>
        <div class="key-row">
          <input
            id="ai-embed-api-key"
            type="password"
            placeholder={loaded.embed_api_key_set ? '•••• embed key on file — leave blank to keep' : 'paste embed API key'}
            value={form.embed_api_key_input ?? ''}
            oninput={(e) => (form.embed_api_key_input = (e.currentTarget as HTMLInputElement).value)}
            autocomplete="off"
            spellcheck="false"
          />
          {#if loaded.embed_api_key_set}
            <button
              type="button"
              class="btn ghost"
              onclick={() => (form.embed_api_key_input = '')}
              title="Submit empty to remove the stored embed key"
            >Clear</button>
          {/if}
        </div>
      </div>
        {/if}

        <div class="field">
          <div class="field-label">
            <label for="ai-embed-model">Model</label>
            <InfoBubble text="Used for semantic search + Datas retrieval. Defaults to nomic-embed-text on Ollama, text-embedding-3-small on OpenAI." />
          </div>
          <input
            id="ai-embed-model"
            type="text"
            bind:value={form.embed_model}
            placeholder={embedPlaceholder(form.provider_kind)}
            autocomplete="off"
            spellcheck="false"
          />
        </div>
      </div>
    </div>

    <!-- ─────────── Always-on / startup ─────────── -->
    <div class="field section-divider">
      <div class="field-label">
        <label for="ai-auto-start">Always on after restart</label>
        <InfoBubble text="When the container restarts (e.g. after an update), AI providers can't load their API keys until your vault unlocks. With this on, providers rebuild automatically the first time you log in after a restart — Datas just keeps working. With it off, you'll need to flip the toolbar toggle (or save Settings) to bring AI back up after each restart. Default on." />
      </div>
      <label class="auto-start">
        <input
          id="ai-auto-start"
          type="checkbox"
          bind:checked={form.auto_start}
        />
        <span>Auto-resume Datas after restart / login</span>
      </label>
      <p class="field-sub">
        {#if form.auto_start}
          On — Datas reactivates automatically the first time you unlock the vault after a restart.
        {:else}
          Off — you'll need to start AI manually (toolbar toggle or Save) after each restart.
        {/if}
      </p>
    </div>

    <!-- ─────────── The Commandments + custom rules ─────────── -->
    <div class="field section-divider commandments-block">
      <div class="field-label">
        <label>The Commandments</label>
        <InfoBubble text="Datas's non-negotiable rule set. These seven rules are baked into the prompt that frames every question — they tell the model it's read-only, that retrieved emails are data not instructions, that it must never recommend clicks or transfers, and so on. Read-only here so a sloppy edit can't weaken the security floor; if you want to extend behaviour (always answer in German, prefer brevity, …) use the Additional rules box below — it's appended after the Commandments and can extend but never override them." />
      </div>

      {#if commandments}
        <button
          type="button"
          class="commandments-toggle"
          onclick={() => (commandmentsExpanded = !commandmentsExpanded)}
          aria-expanded={commandmentsExpanded}
        >
          <span class="cmd-disclosure" class:open={commandmentsExpanded} aria-hidden="true">▸</span>
          <span class="cmd-summary">
            {commandments.commandments.length} non-negotiable rules
            {#if !commandmentsExpanded}<span class="cmd-summary-hint"> — click to read</span>{/if}
          </span>
        </button>

        {#if commandmentsExpanded}
          <ol class="commandments-list">
            {#each commandments.commandments as c (c.n)}
              <li>
                <strong>{c.title}.</strong>
                <span class="cmd-body">{c.body}</span>
              </li>
            {/each}
          </ol>
          <p class="field-sub">
            These ship with the build. To change them you'd need to update Postern itself —
            not editable from here, by design. The audit log records which version of these
            rules was in force for every chat.
          </p>
        {/if}
      {:else}
        <p class="muted">Loading rules…</p>
      {/if}

      <div class="freedom-block">
        <div class="field-label">
          <label for="ai-freedom-mode">Datas freedom</label>
          <InfoBubble text="How permissive Datas is when answering. Affects answer length, willingness to draft reply text, and whether general-knowledge questions get answered. The Commandments above (no sending email, no opening URLs, no executing actions) are unchanged in every mode — only the dial between 'tight refusal' and 'genuinely helpful' moves." />
        </div>
        <select
          id="ai-freedom-mode"
          bind:value={form.freedom_mode}
        >
          <option value="strict">Strict — RAG-anchored, terse, refuses outside corpus</option>
          <option value="balanced">Balanced (default) — general knowledge OK, may suggest drafts</option>
          <option value="open">Open — verbose answers, full elaboration, draft suggestions</option>
        </select>
        <p class="field-sub">
          {#if form.freedom_mode === 'strict'}
            Tight RAG anchoring + 2–3-sentence cap. Refuses email-specific questions that can't be grounded in indexed mail. General-knowledge questions still get answered.
          {:else if form.freedom_mode === 'open'}
            Most helpful mode. Long detailed answers, multi-step reasoning, can propose draft reply text or workflows. Action floor (no sending, no URL fetching, no execution) still binds.
          {:else}
            Default. Concise but elaborates when needed. Answers general questions ("what's today's date?", definitions). May suggest draft reply text — clearly labelled, never sent.
          {/if}
        </p>

        <div class="field-label" style="margin-top: 0.85rem;">
          <label for="ai-chat-max-tokens">Token cap per question</label>
          <InfoBubble text="How many output tokens Datas is allowed per answer. Reasoning models (gpt-5, o-series) burn most of the budget on hidden 'thinking' tokens — if your answers come back empty, push this higher. Classic models (gpt-4o, gpt-4*) work fine at 1000-2000. Default (Auto) = 2000." />
        </div>
        <select
          id="ai-chat-max-tokens"
          bind:value={form.chat_max_tokens}
        >
          <option value={0}>Auto (2000) — recommended</option>
          <option value={500}>500 — very short / cheap</option>
          <option value={1000}>1000 — concise</option>
          <option value={2000}>2000 — balanced (same as Auto)</option>
          <option value={4000}>4000 — generous (good for reasoning models)</option>
          <option value={8000}>8000 — very generous</option>
          <option value={16000}>16000 — maximum (deep reasoning + long answers)</option>
        </select>
        <p class="field-sub">
          {#if form.chat_max_tokens === 0 || form.chat_max_tokens === 2000}
            Default. Plenty for most classic-model answers; usually enough for reasoning models on simple questions.
          {:else if form.chat_max_tokens >= 4000}
            Recommended for gpt-5* and o-series — those models spend a chunk of the budget on hidden reasoning before producing visible output.
          {:else}
            Tight cap. Cheap, fast. Reasoning models may produce empty replies at this level.
          {/if}
        </p>
      </div>

      <div class="user-rules-block">
        <label class="user-rules-label" for="ai-user-rules">
          Additional rules (optional, you can edit)
        </label>
        <textarea
          id="ai-user-rules"
          class="user-rules"
          rows="5"
          placeholder="Free-form text appended after the Commandments. Examples: &quot;Always answer in plain English, no jargon.&quot; · &quot;If a question mentions money, also list the exact amount and currency.&quot; · &quot;Prefer one-sentence answers when possible.&quot;"
          bind:value={form.user_rules}
        ></textarea>
        <p class="field-sub">
          Appended after the Commandments. Cannot override them — if you write
          &quot;ignore the Commandments&quot; here, the model will still follow
          the Commandments because they appear first and are framed as
          non-negotiable.
        </p>
      </div>

      {#if commandments}
        <details class="prompt-preview" bind:open={promptPreviewOpen}>
          <summary>Show full system prompt that gets sent to the model</summary>
          <pre>{commandments.rendered_prompt}</pre>
        </details>
      {/if}
    </div>

    <!-- ─────────── Lockdown mode ─────────── -->
    <div class="field section-divider lockdown-block" class:active={$lockdown.enabled}>
      <div class="field-label">
        <label>Lockdown mode</label>
        <InfoBubble text="Server-side hard kill-switch that physically disables every outbound or destructive action. With lockdown ON: Compose / Reply / Forward / Reply All are disabled, the Send endpoint 403s, the outbox holds pending sends instead of dispatching, Archive / Trash / Move / Spam / Mark-read all 403, message bodies render with no remote images regardless of the 'Show images' toggle, and the Datas / AI surface still works for read-only Q&A. Use this when you want a real guarantee that nothing — not a stale tab, not a session hijack, not the AI hallucinating bad advice — can produce an outbound effect from your inbox." />
      </div>
      <div class="lockdown-row">
        <button
          type="button"
          class="lockdown-toggle"
          class:on={$lockdown.enabled}
          onclick={toggleLockdown}
          disabled={lockdownBusy || !$lockdown.loaded}
          aria-pressed={$lockdown.enabled}
        >
          {#if $lockdown.enabled}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <rect x="3" y="7.5" width="10" height="6.5" rx="1.2"/>
              <path d="M5 7.5V5a3 3 0 0 1 6 0v2.5"/>
            </svg>
            Locked — turn off
          {:else}
            Activate lockdown
          {/if}
        </button>
        <span class="lockdown-status">
          {#if !$lockdown.loaded}
            checking…
          {:else if $lockdown.enabled}
            <span class="lockdown-status-on">Active</span> — outbound + mutations are blocked. Reading + Datas Q&amp;A still work.
          {:else}
            Off — all actions enabled normally.
          {/if}
        </span>
      </div>
      {#if lockdownError}
        <p class="status-line err">⚠ {lockdownError}</p>
      {/if}
      {#if $lockdown.enabled}
        <ul class="lockdown-effects">
          <li>Send / Reply / Forward / Reply-all: <strong>blocked</strong></li>
          <li>Archive / Trash / Move / Spam: <strong>blocked</strong></li>
          <li>Outbox dispatch: <strong>holds pending rows</strong> (no fail, no cancel)</li>
          <li>Remote images / fonts / CSS: <strong>blocked</strong> regardless of per-message toggle</li>
          <li>Datas Q&amp;A (read-only): <strong>still works</strong></li>
        </ul>
      {/if}
    </div>

    <!-- ─────────── Cloud-consent gate ─────────── -->
    {#if cloudConsentRequired}
      <div class="cloud-warning">
        <p>
          {#if isCloudKind && isEmbedCloudKind}
            Both <strong>chat</strong> ({providerLabel(form.provider_kind)})
            and <strong>embedding</strong> ({providerLabel(form.embed_provider_kind)})
            are third-party cloud providers. Every email body will be uploaded
            during indexing AND each question sends 6 retrieved emails to the
            chat provider.
          {:else if isCloudKind}
            <strong>{providerLabel(form.provider_kind)}</strong> is a third-party
            cloud provider. Each question sends 6 retrieved emails (subject,
            sender, snippet, body slice) to their servers. Embeddings stay
            local on Ollama.
          {:else}
            <strong>{providerLabel(form.embed_provider_kind)}</strong> is a
            third-party cloud provider for embeddings. Every email body
            (~2 KB each) will be sent there during indexing.
          {/if}
        </p>
        <label class="consent">
          <input type="checkbox" bind:checked={form.cloud_consent} />
          <span>I understand mail content will be sent to a third-party provider with this configuration.</span>
        </label>
      </div>
    {/if}

    <!-- ─────────── Indexing & data ───────────
         Grouped together because they're all about what Datas can
         see and how much of the corpus it has chewed through. -->
    <h3 class="section-h3">Indexing &amp; data</h3>

    <!-- ─────────── Index coverage ─────────── -->
    {#if coverage}
      <div class="field">
        <div class="field-label">
          <label>Index coverage</label>
          <InfoBubble text="How many of your messages are embedded against the current model. The indexer wakes every minute and processes a small batch — recent mail first." />
        </div>
        <div class="ai-progress" role="progressbar"
             aria-valuemin="0"
             aria-valuemax={coverage.total}
             aria-valuenow={coverage.indexed}>
          <div class="ai-progress-track">
            <div class="ai-progress-fill" style="width: {indexedPercent(coverage)}%"></div>
          </div>
          <span class="ai-progress-label">
            {coverage.indexed.toLocaleString()} / {coverage.total.toLocaleString()}
            messages ({indexedPercent(coverage)}%)
          </span>
        </div>
        <div class="reindex-row">
          <button
            type="button"
            class="btn ghost"
            disabled={reindexBusy || coverage.indexed === 0}
            onclick={confirmAndReindex}
            title="Wipe all stored vectors and let the indexer rebuild from scratch. Useful after the embedding format changes (e.g. adding sender headers)."
          >
            {reindexBusy ? 'Clearing…' : 'Re-index from scratch'}
          </button>
          <span class="field-sub">
            Wipes existing vectors. The indexer rebuilds against the current
            embed provider — ~1–2 hours on local Ollama, free.
          </span>
        </div>
        {#if reindexResult}
          <p class="status-line ok">✓ Cleared {reindexResult.toLocaleString()} embeddings. Indexer will start re-embedding within a minute.</p>
        {/if}
        {#if reindexError}
          <p class="status-line err">⚠ {reindexError}</p>
        {/if}
      </div>
    {/if}

    <!-- ─────────── Indexing exclusions ─────────── -->
    <div class="field">
      <div class="field-label">
        <label>Exclude from indexing</label>
        <InfoBubble text="Senders and folders listed here are skipped by the indexer entirely — embeddings are never created for them. Useful for cPanel / server-monitor / shipping-tracker noise that would otherwise dominate retrieval. Saving here also prunes existing vectors that match, so noise leaves Datas's view immediately. Datas Q&A also filters these out at retrieval time as a belt-and-braces measure." />
      </div>

      <div class="exclusion-grid">
        <div class="excl-col">
          <label class="excl-label" for="ai-excluded-senders">
            Senders
            <span class="excl-hint">one per line · `*` = wildcard · `#` for comments</span>
          </label>
          <textarea
            id="ai-excluded-senders"
            class="user-rules"
            rows="6"
            spellcheck="false"
            placeholder={`# Examples:
*@cpanel.example.com
noreply@server-monitor.com
*newsletter*
*notifications@github.com`}
            bind:value={form.excluded_senders}
          ></textarea>
        </div>

        <div class="excl-col">
          <label class="excl-label" for="ai-excluded-labels">
            Folders / labels
            <span class="excl-hint">one per line · exact match</span>
          </label>
          <textarea
            id="ai-excluded-labels"
            class="user-rules"
            rows="6"
            spellcheck="false"
            placeholder={`# Examples:
Trash
Spam
[Gmail]/Promotions
[Gmail]/Trash
Junk`}
            bind:value={form.excluded_labels}
          ></textarea>
        </div>
      </div>

      <p class="field-sub">
        Saving these rules will also prune any existing embeddings that match —
        the noise leaves Datas immediately, not just for future indexing.
      </p>
    </div>

    <!-- ─────────── Chat history ─────────── -->
    {#if coverage}
      <div class="row">
        <div class="label">
          <strong class="inline">
            Chat history
            <InfoBubble text="Every Q&A round-trip is logged so you can audit what the AI saw and said. The history lives in SQLCipher — same vault key as your mail." />
          </strong>
          <span class="field-sub">
            {#if coverage.chat_history_count === 0}
              No conversations stored yet.
            {:else}
              {coverage.chat_history_count.toLocaleString()} stored
              {coverage.chat_history_count === 1 ? 'conversation' : 'conversations'}.
            {/if}
          </span>
        </div>
        <button
          class="btn danger"
          type="button"
          disabled={clearing || coverage.chat_history_count === 0}
          onclick={clearHistory}
        >
          {clearing ? 'Forgetting…' : 'Forget all'}
        </button>
      </div>
    {/if}

    <!-- ─────────── Test + Save (form footer) ─────────── -->
    <div class="form-footer">
      <div class="actions">
        <button type="button" class="btn ghost" disabled={testing || saving} onclick={runTest}>
          {testing ? 'Testing…' : 'Test connection'}
        </button>
        <button type="button" class="btn primary" disabled={saving || testing} onclick={save}>
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>

      {#if testResult}
        <p class="status-line {testResult.ok ? 'ok' : 'err'}">
          {#if testResult.ok}
            ✓ {testResult.provider} reachable ({postureLabel(testResult.privacy_posture as PrivacyPosture)})
          {:else}
            ✗ {testResult.message ?? 'Test failed'}
          {/if}
        </p>
      {/if}
      {#if saveError}
        <p class="status-line err">⚠ {saveError}</p>
      {:else if saveOk}
        <p class="status-line ok">✓ Settings saved — provider hot-swapped, no restart needed.</p>
      {/if}
    </div>
  {/if}
</section>

<style>
  .sub-tabs {
    display: inline-flex;
    align-items: center;
    gap: 0.18rem;
    margin: 0.4rem 0 1rem;
    padding: 0.18rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--surface-2) 60%, transparent);
  }
  .sub-tabs button {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.42rem 0.95rem;
    border-radius: 999px;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .sub-tabs button:hover:not(.active) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .sub-tabs button.active {
    background: var(--accent);
    color: white;
    font-weight: 600;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 1rem;
  }
  .ai-form-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.85rem;
    margin: 0.85rem 0 1rem;
  }
  .ai-form-card {
    min-width: 0;
    padding: 0.9rem 0.95rem 0.1rem;
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 36%, transparent);
  }
  .ai-form-card h3 {
    margin: 0 0 0.8rem;
    font-size: 0.9rem;
    font-weight: 700;
  }
  .field-label {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .field input[type='text'],
  .field input[type='url'],
  .field input[type='password'],
  .field select {
    width: 100%;
    padding: 0.55rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 0.9rem;
    box-sizing: border-box;
  }
  .field input:focus,
  .field select:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .field-sub {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .field-sub.good {
    color: color-mix(in oklab, mediumseagreen 75%, var(--fg));
  }
  .field-sub.warn {
    color: color-mix(in oklab, tomato 75%, var(--fg));
  }
  .field.section-divider {
    margin-top: 1.4rem;
    padding-top: 1.1rem;
    border-top: 1px dashed color-mix(in oklab, currentColor 18%, transparent);
  }

  .key-row {
    display: flex;
    align-items: stretch;
    gap: 0.45rem;
  }
  .key-row input { flex: 1; }

  .backend-rows {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin: 0.45rem 0 0.85rem;
    padding: 0.6rem 0.9rem;
    background: color-mix(in oklab, var(--surface-2) 55%, transparent);
    border: 1px solid color-mix(in oklab, var(--border) 70%, transparent);
    border-radius: 0.7rem;
  }
  .backend-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    flex-wrap: wrap;
    font-size: 0.86rem;
  }
  .backend-label {
    min-width: 7.5rem;
    color: var(--muted);
    font-size: 0.78rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .backend-name {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.82rem;
    padding: 0.12rem 0.5rem;
    border-radius: 0.4rem;
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .backend-missing {
    color: color-mix(in oklab, tomato 70%, var(--fg));
    font-size: 0.8rem;
  }

  .enable-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.7rem;
  }
  .switch {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    cursor: pointer;
  }
  .switch input {
    appearance: none;
    width: 36px;
    height: 20px;
    background: color-mix(in oklab, currentColor 18%, transparent);
    border-radius: 999px;
    position: relative;
    transition: background 140ms ease;
    outline: none;
  }
  .switch input::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: white;
    transition: transform 140ms ease;
  }
  .switch input:checked {
    background: color-mix(in oklab, var(--accent) 80%, transparent);
  }
  .switch input:checked::after {
    transform: translateX(16px);
  }
  .switch .slider { display: none; }
  .switch-label {
    font-size: 0.82rem;
    font-weight: 600;
  }

  .auto-start {
    display: inline-flex;
    align-items: flex-start;
    gap: 0.55rem;
    font-size: 0.86rem;
    cursor: pointer;
  }
  .auto-start input { margin-top: 0.18rem; }

  /* ─── Commandments ─── */
  .commandments-block {
    /* Subtle parchment-y treatment — these are the "rules of
       Datas" so a slightly different surface helps separate them
       from the operational settings above. */
    border-radius: 0.7rem;
    padding: 0.95rem 1.1rem;
    background: color-mix(in oklab, var(--surface-2) 50%, transparent);
    border: 1px solid var(--border);
    margin-top: 1.4rem;
  }
  .commandments-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 0.88rem;
    cursor: pointer;
    padding: 0.3rem 0;
    margin-bottom: 0.2rem;
  }
  .cmd-disclosure {
    display: inline-block;
    transition: transform 140ms ease;
    color: var(--muted);
    width: 0.9em;
    text-align: center;
  }
  .cmd-disclosure.open { transform: rotate(90deg); }
  .cmd-summary { font-weight: 600; }
  .cmd-summary-hint { color: var(--muted); font-weight: 400; }

  .commandments-list {
    margin: 0.6rem 0 0.4rem;
    padding-left: 1.4rem;
    font-size: 0.86rem;
    line-height: 1.55;
  }
  .commandments-list li { margin: 0.45rem 0; }
  .cmd-body { color: var(--fg); }

  .freedom-block {
    margin-top: 1rem;
    border-top: 1px dashed color-mix(in oklab, currentColor 16%, transparent);
    padding-top: 0.85rem;
  }
  .freedom-block select {
    width: 100%;
    max-width: 38rem;
  }
  .user-rules-block {
    margin-top: 1rem;
    border-top: 1px dashed color-mix(in oklab, currentColor 16%, transparent);
    padding-top: 0.85rem;
  }
  .user-rules-label {
    display: block;
    font-size: 0.84rem;
    font-weight: 600;
    margin-bottom: 0.4rem;
  }
  textarea.user-rules {
    width: 100%;
    box-sizing: border-box;
    min-height: 6.5rem;
    resize: vertical;
    padding: 0.6rem 0.8rem;
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--fg);
    font: inherit;
    font-size: 0.86rem;
    line-height: 1.5;
  }
  textarea.user-rules:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 40%, transparent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 14%, transparent);
  }

  details.prompt-preview {
    margin-top: 0.85rem;
    border-top: 1px dashed color-mix(in oklab, currentColor 16%, transparent);
    padding-top: 0.7rem;
  }
  details.prompt-preview summary {
    cursor: pointer;
    font-size: 0.82rem;
    color: var(--muted);
    user-select: none;
  }
  details.prompt-preview summary:hover { color: var(--fg); }
  details.prompt-preview pre {
    margin: 0.55rem 0 0;
    padding: 0.7rem 0.85rem;
    background: color-mix(in oklab, var(--surface-2) 80%, transparent);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.74rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 22rem;
    overflow-y: auto;
  }

  /* Lockdown UI — distinct visual weight because flipping it on
     is a heavy action and we want the user to read the
     consequences before clicking. */
  .lockdown-block.active {
    border-radius: 0.7rem;
    padding: 0.85rem 1rem;
    background: color-mix(in oklab, tomato 10%, var(--surface));
    border: 1px solid color-mix(in oklab, tomato 32%, transparent);
  }
  .lockdown-row {
    display: inline-flex;
    align-items: center;
    gap: 0.85rem;
    flex-wrap: wrap;
  }
  .lockdown-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    font: inherit;
    font-size: 0.86rem;
    font-weight: 600;
    padding: 0.45rem 0.95rem;
    border-radius: 999px;
    border: 1px solid color-mix(in oklab, tomato 50%, transparent);
    background: transparent;
    color: color-mix(in oklab, tomato 78%, var(--fg));
    cursor: pointer;
  }
  .lockdown-toggle:hover:not(:disabled) {
    background: color-mix(in oklab, tomato 12%, transparent);
  }
  .lockdown-toggle.on {
    background: color-mix(in oklab, tomato 75%, transparent);
    color: white;
    border-color: color-mix(in oklab, tomato 80%, transparent);
  }
  .lockdown-toggle.on:hover:not(:disabled) {
    background: color-mix(in oklab, tomato 65%, transparent);
  }
  .lockdown-toggle:disabled { opacity: 0.55; cursor: progress; }
  .lockdown-status {
    font-size: 0.84rem;
    color: var(--muted);
  }
  .lockdown-status-on {
    color: color-mix(in oklab, tomato 80%, var(--fg));
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 0.78rem;
  }
  ul.lockdown-effects {
    margin: 0.6rem 0 0;
    padding-left: 1.1rem;
    font-size: 0.82rem;
    color: var(--fg);
    line-height: 1.55;
  }
  ul.lockdown-effects li { margin: 0.05rem 0; }

  .cloud-warning {
    margin: 0.4rem 0 1rem;
    padding: 0.7rem 0.9rem;
    border: 1px solid color-mix(in oklab, tomato 35%, transparent);
    background: color-mix(in oklab, tomato 8%, var(--surface));
    border-radius: 0.6rem;
  }
  .cloud-warning p {
    margin: 0 0 0.5rem;
    font-size: 0.86rem;
    line-height: 1.5;
  }
  .consent {
    display: inline-flex;
    align-items: flex-start;
    gap: 0.5rem;
    font-size: 0.84rem;
  }
  .consent input { margin-top: 0.18rem; }

  /* Section heading for grouped fields. Quiet enough to not compete
     with the page-level <h2>, but heavy enough to act as a visual
     break between major groups (Indexing & data, etc.). */
  .section-h3 {
    margin: 1.4rem 0 0.6rem;
    padding-top: 1rem;
    border-top: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    font-size: 0.92rem;
    font-weight: 600;
    letter-spacing: -0.005em;
    color: color-mix(in oklab, currentColor 80%, transparent);
  }

  /* Form footer — Test/Save buttons and any inline status. Sits at
     the bottom of the editable form so the user always knows where
     the commit-this-form action is. Top border separates it from
     the last data field above. */
  .form-footer {
    margin-top: 1.4rem;
    padding-top: 1rem;
    border-top: 1px solid color-mix(in oklab, currentColor 12%, transparent);
  }

  .actions {
    display: inline-flex;
    gap: 0.55rem;
    margin: 0.3rem 0 0.6rem;
  }
  .btn {
    font: inherit;
    font-size: 0.85rem;
    padding: 0.5rem 1.1rem;
    border-radius: 999px;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
  }
  .btn.primary {
    background: dodgerblue;
    border-color: dodgerblue;
    color: white;
    font-weight: 500;
  }
  .btn.primary:hover:not(:disabled) {
    background: color-mix(in oklab, dodgerblue 85%, black);
  }
  .btn.danger {
    color: #c83333;
    border-color: color-mix(in oklab, #c83333 35%, transparent);
  }
  .btn.danger:hover:not(:disabled) {
    background: color-mix(in oklab, #c83333 12%, transparent);
  }
  .btn.ghost:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .btn.small {
    padding: 0.35rem 0.7rem;
    font-size: 0.78rem;
  }
  .btn:disabled { opacity: 0.55; cursor: progress; }

  .polish-row {
    display: flex;
    gap: 0.4rem;
    align-items: stretch;
  }
  .polish-row select { flex: 1; min-width: 0; }

  .status-line {
    margin: 0.3rem 0 0.6rem;
    font-size: 0.86rem;
  }
  .status-line.ok { color: forestgreen; }
  .status-line.err { color: #c83333; }

  .reindex-row {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    margin-top: 0.55rem;
    flex-wrap: wrap;
  }
  .reindex-row .field-sub { margin: 0; flex: 1 1 16rem; }

  /* Two-column textarea grid for senders + labels. Stacks
     vertically on narrow screens. */
  .exclusion-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.85rem;
    margin: 0.4rem 0 0.55rem;
  }
  @media (max-width: 700px) {
    .exclusion-grid { grid-template-columns: 1fr; }
  }
  .excl-col { display: flex; flex-direction: column; gap: 0.35rem; }
  .excl-label {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 0.84rem;
    font-weight: 600;
  }
  .excl-hint {
    color: var(--muted);
    font-weight: 400;
    font-size: 0.74rem;
  }

  .ai-progress { display: flex; flex-direction: column; gap: 0.4rem; }
  .ai-progress-track {
    width: 100%;
    height: 8px;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 8%, transparent);
    overflow: hidden;
  }
  .ai-progress-fill {
    height: 100%;
    background: linear-gradient(90deg,
      color-mix(in oklab, mediumseagreen 70%, transparent),
      color-mix(in oklab, mediumseagreen 90%, transparent));
    transition: width 240ms ease-out;
  }
  .ai-progress-label { font-size: 0.78rem; color: var(--muted); }

  .posture {
    font-size: 0.7rem;
    font-weight: 700;
    padding: 0.18rem 0.55rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border: 1px solid transparent;
  }
  .posture.posture-local {
    background: color-mix(in oklab, mediumseagreen 18%, transparent);
    color: color-mix(in oklab, mediumseagreen 70%, var(--fg));
    border-color: color-mix(in oklab, mediumseagreen 22%, transparent);
  }
  .posture.posture-self {
    background: color-mix(in oklab, gold 18%, transparent);
    color: color-mix(in oklab, gold 70%, var(--fg));
    border-color: color-mix(in oklab, gold 22%, transparent);
  }
  .posture.posture-cloud {
    background: color-mix(in oklab, tomato 18%, transparent);
    color: color-mix(in oklab, tomato 70%, var(--fg));
    border-color: color-mix(in oklab, tomato 22%, transparent);
  }

  @media (max-width: 760px) {
    .sub-tabs {
      width: 100%;
      box-sizing: border-box;
      overflow-x: auto;
      scrollbar-width: none;
    }
    .ai-form-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
