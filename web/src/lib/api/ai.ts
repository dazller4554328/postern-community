/** AI slice — compose-pane Polish + Dictate + Settings → AI config.
 *  Whole module is dead in community builds; routes return 400. */

import { request } from './_client';
import type {
  AiModelsResponse,
  AiRewriteRequest,
  AiRewriteResponse,
  AiSettingsDto,
  AiSettingsTest,
  AiSettingsTestResult,
  AiSettingsUpdate,
  AiStatus
} from '../api';

export const aiApi = {
  aiStatus: () => request<AiStatus>('/api/ai/status'),
  /** One-shot rewrite of a single block of user-authored text. No
   *  retrieval, no email context — sends only the supplied draft to
   *  the configured chat model. Used by the compose pane's "Polish"
   *  button so token spend stays bounded by what the user typed. */
  aiRewrite: (body: AiRewriteRequest) =>
    request<AiRewriteResponse>('/api/ai/rewrite', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /** Snapshot what the configured chat provider has installed /
   *  exposed (Ollama tags, OpenAI /v1/models, Anthropic /v1/models).
   *  Used by Settings → AI to populate the Polish-model dropdown so
   *  the user picks from real options instead of typing freehand. */
  aiListModels: () => request<AiModelsResponse>('/api/ai/models'),
  /** Voice dictation: upload an audio blob (recorded in the browser
   *  via MediaRecorder), get a transcript back. Server forwards to
   *  the configured chat provider's transcribe endpoint — only
   *  OpenAI (Whisper) is supported today; other providers return
   *  400. Audio routes through Postern's outbound (your VPN if
   *  configured), never browser-direct to a third party. */
  aiTranscribe: async (
    blob: Blob
  ): Promise<{
    text: string;
    provider: string;
    elapsed_ms: number;
    audio_bytes: number;
  }> => {
    const form = new FormData();
    form.append('file', blob, 'audio.webm');
    const r = await fetch('/api/ai/transcribe', {
      method: 'POST',
      body: form,
      credentials: 'include'
    });
    if (!r.ok) {
      const text = await r.text().catch(() => '');
      throw new Error(text || `transcribe failed: ${r.status}`);
    }
    return r.json();
  },
  /** Persisted Settings → AI config. The api key never travels in
   *  the response — only `api_key_set` indicates whether one is on
   *  file. Vault must be unlocked to save (POST), but GET is open
   *  so the panel can render even before unlock. */
  aiGetSettings: () => request<AiSettingsDto>('/api/ai/settings'),
  aiUpdateSettings: (body: AiSettingsUpdate) =>
    request<AiSettingsDto>('/api/ai/settings', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  aiTestSettings: (body: AiSettingsTest) =>
    request<AiSettingsTestResult>('/api/ai/test', {
      method: 'POST',
      body: JSON.stringify(body)
    }),
  /** Quick on/off switch — does not touch provider/model/key. Off
   *  releases the chat provider atomically so no outbound API calls
   *  happen until it's flipped back on. */
  aiSetEnabled: (enabled: boolean) =>
    request<AiSettingsDto>('/api/ai/enabled', {
      method: 'POST',
      body: JSON.stringify({ enabled })
    })
};
