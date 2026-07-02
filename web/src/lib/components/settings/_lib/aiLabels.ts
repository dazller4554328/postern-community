import type { AiProviderKind, PrivacyPosture } from '$lib/api';

/// Hosted OpenAI-compatible providers — keeping this in one place
/// so the "needs cloud consent" gate stays consistent.
export function isHostedCompat(url: string): boolean {
  const lower = url.toLowerCase();
  return [
    'api.x.ai',
    'api.groq.com',
    'api.together.xyz',
    'api.perplexity.ai',
    'api.deepseek.com',
    'api.mistral.ai',
  ].some((m) => lower.includes(m));
}

export function postureLabel(p: PrivacyPosture | null | undefined): string {
  if (!p) return 'unknown';
  if (p === 'local_only') return 'Local only';
  if (p === 'user_controlled_remote') return 'Your remote box';
  return 'Third-party cloud';
}

export function postureClass(p: PrivacyPosture | null | undefined): string {
  if (!p) return '';
  if (p === 'local_only') return 'posture-local';
  if (p === 'user_controlled_remote') return 'posture-self';
  return 'posture-cloud';
}

export function providerLabel(k: AiProviderKind): string {
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

export function chatPlaceholder(k: AiProviderKind): string {
  if (k === 'anthropic') return 'claude-sonnet-4-6';
  if (k === 'openai') return 'gpt-4o-mini';
  if (k === 'openai_compat') return 'grok-beta';
  return 'llama3.1:8b-instruct-q4_K_M';
}
