<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type AiProviderKind,
    type AiSettingsDto,
    type AiSettingsTestResult,
    type AiStatus,
    type PrivacyPosture
  } from '$lib/api';
  import InfoBubble from '$lib/components/InfoBubble.svelte';
  import {
    isHostedCompat,
    postureLabel,
    postureClass,
    providerLabel,
    chatPlaceholder,
  } from './_lib/aiLabels';

  /// What the user is editing on this page. Diverges from `loaded`
  /// once they touch any field; saved only on submit.
  interface FormState {
    enabled: boolean;
    provider_kind: AiProviderKind;
    chat_model: string;
    base_url: string;
    /// User typed a new key this session. "" = clear,
    /// null = "leave the existing key in place".
    api_key_input: string | null;
    cloud_consent: boolean;
    auto_start: boolean;
    /// Optional model override used by the compose-pane Polish
    /// rewrite. Empty string = inherit chat_model.
    polish_chat_model: string;
  }

  let loaded = $state<AiSettingsDto | null>(null);
  let status = $state<AiStatus | null>(null);
  let form = $state<FormState>({
    enabled: true,
    provider_kind: 'ollama',
    chat_model: '',
    base_url: '',
    api_key_input: null,
    cloud_consent: false,
    auto_start: true,
    polish_chat_model: ''
  });

  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveOk = $state(false);
  let testResult = $state<AiSettingsTestResult | null>(null);
  let testing = $state(false);
  let loading = $state(true);

  /// Models the active chat provider currently has installed —
  /// drives the Polish-model dropdown.
  let modelList = $state<string[]>([]);
  let modelListLoading = $state(false);
  let modelListError = $state<string | null>(null);
  let modelListProvider = $state<string>('');

  /// Chat-model field mode: 'pick' (dropdown) or 'custom' (free-text).
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

  let isCloudKind = $derived(
    form.provider_kind === 'anthropic' ||
      form.provider_kind === 'openai' ||
      (form.provider_kind === 'openai_compat' && isHostedCompat(form.base_url))
  );
  let needsApiKey = $derived(form.provider_kind !== 'ollama');
  let needsBaseUrl = $derived(form.provider_kind === 'openai_compat');
  let supportsBaseUrlOverride = $derived(form.provider_kind === 'ollama');
  let cloudConsentRequired = $derived(isCloudKind);

  // isHostedCompat / postureLabel / postureClass / providerLabel /
  // chatPlaceholder live in ./_lib/aiLabels.ts.

  async function refresh() {
    loading = true;
    try {
      const [settings, st] = await Promise.all([api.aiGetSettings(), api.aiStatus()]);
      loaded = settings;
      status = st;
      form = {
        enabled: settings.enabled,
        provider_kind: settings.provider_kind,
        chat_model: settings.chat_model,
        base_url: settings.base_url ?? '',
        api_key_input: null,
        cloud_consent: settings.cloud_consent,
        auto_start: settings.auto_start,
        polish_chat_model: settings.polish_chat_model ?? ''
      };
      void refreshModels();
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
      api_key_input: null,
      base_url: next === 'openai_compat' ? form.base_url : '',
      cloud_consent: next === 'ollama' ? false : form.cloud_consent
    };
    testResult = null;
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
        'Tick the cloud-consent confirmation — this configuration sends draft text off your box.';
      return;
    }
    saving = true;
    try {
      const next = await api.aiUpdateSettings({
        enabled: form.enabled,
        provider_kind: form.provider_kind,
        chat_model: form.chat_model || undefined,
        base_url: form.base_url || null,
        api_key: form.api_key_input,
        cloud_consent: form.cloud_consent,
        auto_start: form.auto_start,
        polish_chat_model: form.polish_chat_model.trim()
      });
      loaded = next;
      status = await api.aiStatus();
      form = {
        ...form,
        api_key_input: null,
        cloud_consent: next.cloud_consent,
        polish_chat_model: next.polish_chat_model ?? ''
      };
      void refreshModels();
      saveOk = true;
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void refresh();
  });
</script>

<section class="panel">
  <div class="section-head">
    <h2>AI</h2>
    <p>
      Pick a backend for the compose-pane Polish and Dictate features.
      Local Ollama keeps every byte on your box; cloud providers
      (Claude, OpenAI, Grok) need an API key and send the draft text
      you ask them to rewrite or transcribe off-box.
    </p>
  </div>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if !loaded}
    <p class="muted">Could not load AI settings.</p>
  {:else}
    <!-- Master enable -->
    <div class="row">
      <div class="label">
        <strong class="inline">
          AI features
          <InfoBubble text="Master switch. When off, the chat provider is unloaded — Polish + Dictate become unavailable until you flip it back on." />
        </strong>
        <span class="field-sub">
          {#if status?.enabled}
            Live — see backend below.
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

    <!-- Live backend status -->
    {#if status?.enabled}
      <div class="backend-rows">
        <div class="backend-row">
          <span class="backend-label">Backend</span>
          <code class="backend-name">{status.provider ?? '—'}</code>
          {#if status.privacy_posture}
            <span class="posture {postureClass(status.privacy_posture)}">
              {postureLabel(status.privacy_posture)}
            </span>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Provider -->
    <div class="field">
      <div class="field-label">
        <label for="ai-provider-kind">Provider</label>
        <InfoBubble text="Where Polish + Dictate's calls go. Local Ollama is private + free but lower-quality. Anthropic + OpenAI / xAI need an API key from their dashboard." />
      </div>
      <select
        id="ai-provider-kind"
        bind:value={form.provider_kind}
        onchange={(e) =>
          onKindChange((e.currentTarget as HTMLSelectElement).value as AiProviderKind)}
      >
        <option value="ollama">{providerLabel('ollama')}</option>
        <option value="anthropic">{providerLabel('anthropic')}</option>
        <option value="openai">{providerLabel('openai')}</option>
        <option value="openai_compat">{providerLabel('openai_compat')}</option>
      </select>
    </div>

    <!-- Base URL (Ollama override / openai_compat) -->
    {#if needsBaseUrl || supportsBaseUrlOverride}
      <div class="field">
        <div class="field-label">
          <label for="ai-base-url">Base URL</label>
          <InfoBubble
            text={needsBaseUrl
              ? 'Required. e.g. https://api.x.ai/v1 for Grok, or http://vllm.lan:8000/v1 for self-hosted.'
              : 'Override where Postern looks for Ollama. Leave blank for the default localhost:11434.'}
          />
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

    <!-- API key -->
    {#if needsApiKey}
      <div class="field">
        <div class="field-label">
          <label for="ai-api-key">API key</label>
          <InfoBubble text="Stored encrypted in the vault, never returned over the network. Leave the field empty to keep the existing key. Type a new value to rotate. Submit empty with Clear to remove." />
        </div>
        <div class="key-row">
          <input
            id="ai-api-key"
            type="password"
            placeholder={loaded.api_key_set
              ? '•••• key on file — leave blank to keep'
              : 'paste your API key'}
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

    <!-- Chat model -->
    <div class="field">
      <div class="field-label">
        <label for="ai-chat-model">Model</label>
        <InfoBubble text="The chat model. Pick from what your provider has installed/exposed; switch to 'Custom…' to type a model id manually for backends whose list endpoint is unreliable." />
      </div>
      <div class="polish-row">
        {#if chatModelMode === 'pick'}
          <select
            id="ai-chat-model"
            bind:value={form.chat_model}
            disabled={modelListLoading}
          >
            <option value=""
              >(default for {providerLabel(form.provider_kind)} — {chatPlaceholder(
                form.provider_kind
              )})</option
            >
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

    <!-- Polish model (compose-pane rewrite) -->
    <div class="field">
      <div class="field-label">
        <label for="ai-polish-model">Polish model</label>
        <InfoBubble text="Used by the compose-pane 'Polish selection' button. Defaults to the same model as above — pick a smaller/cheaper one (e.g. gpt-4o-mini, llama3.2:3b) here if you want polish to be quick. Leave blank to inherit." />
      </div>
      <div class="polish-row">
        <select
          id="ai-polish-model"
          bind:value={form.polish_chat_model}
          disabled={modelListLoading}
        >
          <option value=""
            >(inherit chat model{form.chat_model ? ` — ${form.chat_model}` : ''})</option
          >
          {#if form.polish_chat_model && !modelList.includes(form.polish_chat_model)}
            <option value={form.polish_chat_model}
              >{form.polish_chat_model} (saved, not in current list)</option
            >
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
          No models reported by {modelListProvider || 'the active provider'} yet — save Settings
          (or click ↻) once the provider is reachable to populate this list.
        </p>
      {:else if modelListProvider}
        <p class="field-sub">
          {modelList.length} model{modelList.length === 1 ? '' : 's'} from {modelListProvider}.
        </p>
      {/if}
    </div>

    <!-- Always-on / startup -->
    <div class="field section-divider">
      <div class="field-label">
        <label for="ai-auto-start">Always on after restart</label>
        <InfoBubble text="When the container restarts, AI providers can't load their API key until your vault unlocks. With this on, the provider rebuilds automatically the first time you log in after a restart. Default on." />
      </div>
      <label class="auto-start">
        <input id="ai-auto-start" type="checkbox" bind:checked={form.auto_start} />
        <span>Auto-resume after restart / login</span>
      </label>
    </div>

    <!-- Cloud-consent gate -->
    {#if cloudConsentRequired}
      <div class="cloud-warning">
        <p>
          <strong>{providerLabel(form.provider_kind)}</strong> is a third-party cloud provider.
          Each Polish or Dictate call sends the relevant draft text or audio to their servers.
        </p>
        <label class="consent">
          <input type="checkbox" bind:checked={form.cloud_consent} />
          <span>I understand draft content will be sent to a third-party provider with this configuration.</span>
        </label>
      </div>
    {/if}

    <!-- Test + Save (form footer) -->
    <div class="form-footer">
      <div class="actions">
        <button
          type="button"
          class="btn ghost"
          disabled={testing || saving}
          onclick={runTest}
        >
          {testing ? 'Testing…' : 'Test connection'}
        </button>
        <button type="button" class="btn primary" disabled={saving || testing} onclick={save}>
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>

      {#if testResult}
        <p class="status-line {testResult.ok ? 'ok' : 'err'}">
          {#if testResult.ok}
            ✓ {testResult.provider} reachable ({postureLabel(
              testResult.privacy_posture as PrivacyPosture
            )})
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 1rem;
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
</style>
