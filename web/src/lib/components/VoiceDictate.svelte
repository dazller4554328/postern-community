<script lang="ts">
  // Voice dictation. Records audio in the browser via MediaRecorder,
  // uploads the captured blob to Postern's /api/ai/transcribe, and
  // appends the returned transcript to the body via onAppend.
  //
  // Why server-side transcription instead of the Web Speech API:
  // browser → Google STT was unreliable (firewalls, browser
  // extensions, "network error reaching the speech service") and
  // sent every audio chunk straight to Google regardless of the
  // user's VPN setup. Routing through Postern means the audio goes
  // browser → Postern (over the existing tunnel / Tailscale Funnel)
  // → OpenAI (over the user's VPN egress, if configured). Same
  // provider footprint as chat — if you trust OpenAI for chat, the
  // audio path inherits that trust.
  //
  // UX flow:
  //   * Click mic → permission prompt → recording starts (red pulse).
  //   * Click mic again → stop, blob uploads to server.
  //   * Spinner while transcribing (~1-3 s for typical email length).
  //   * Transcript text lands in the body via onAppend.
  // No interim/streaming preview — Whisper is record-then-transcribe.

  import { onDestroy } from 'svelte';
  import { api } from '$lib/api';

  interface Props {
    /** Called once with the full transcript after server returns. */
    onAppend: (chunk: string) => void;
  }

  let { onAppend }: Props = $props();

  // Feature-detect MediaRecorder + getUserMedia. Firefox + every
  // modern browser have these; older Safari (< 14) doesn't, but
  // that's well below our floor.
  const supported =
    typeof window !== 'undefined' &&
    typeof window.MediaRecorder !== 'undefined' &&
    !!navigator.mediaDevices?.getUserMedia;

  let recording = $state(false);
  let transcribing = $state(false);
  let err = $state<string | null>(null);
  let elapsedSec = $state(0);
  let recorder: MediaRecorder | null = null;
  let stream: MediaStream | null = null;
  let chunks: Blob[] = [];
  let timerId: ReturnType<typeof setInterval> | null = null;
  let startedAt = 0;

  // Pick the MediaRecorder mimeType the browser actually supports.
  // OpenAI's Whisper endpoint accepts webm, m4a, mp4, ogg — most
  // browsers default to webm/opus which Whisper handles natively.
  function pickMime(): string {
    const candidates = [
      'audio/webm;codecs=opus',
      'audio/webm',
      'audio/mp4',
      'audio/ogg;codecs=opus',
      'audio/ogg'
    ];
    for (const m of candidates) {
      if (typeof MediaRecorder !== 'undefined' && MediaRecorder.isTypeSupported(m)) {
        return m;
      }
    }
    return ''; // empty = browser default
  }

  async function start() {
    if (!supported || recording || transcribing) return;
    err = null;
    chunks = [];
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          // Modest constraints — no point capturing 48 kHz when
          // Whisper down-samples to 16 kHz internally. Smaller
          // bitrate = faster upload.
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true
        }
      });
    } catch (e) {
      err =
        e instanceof Error && /denied|NotAllowed/i.test(e.message)
          ? 'Microphone permission denied.'
          : 'Could not access the microphone.';
      return;
    }

    const mime = pickMime();
    try {
      recorder = mime ? new MediaRecorder(stream, { mimeType: mime }) : new MediaRecorder(stream);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
      stopStreamTracks();
      return;
    }
    recorder.ondataavailable = (ev: BlobEvent) => {
      if (ev.data && ev.data.size > 0) chunks.push(ev.data);
    };
    recorder.onstop = () => {
      void uploadAndTranscribe();
    };
    recorder.start();
    recording = true;
    startedAt = Date.now();
    elapsedSec = 0;
    timerId = setInterval(() => {
      elapsedSec = Math.floor((Date.now() - startedAt) / 1000);
    }, 250);
  }

  function stop() {
    if (!recording || !recorder) return;
    recording = false;
    if (timerId) {
      clearInterval(timerId);
      timerId = null;
    }
    try {
      recorder.stop();
    } catch {
      /* already stopped */
    }
  }

  async function uploadAndTranscribe() {
    stopStreamTracks();
    const blobType = recorder?.mimeType || 'audio/webm';
    recorder = null;
    if (chunks.length === 0) {
      // Quick click? Nothing recorded — say so and bail.
      err = 'No audio captured — try holding the mic open longer.';
      return;
    }
    const blob = new Blob(chunks, { type: blobType });
    chunks = [];
    if (blob.size === 0) {
      err = 'No audio captured.';
      return;
    }
    transcribing = true;
    try {
      const r = await api.aiTranscribe(blob);
      if (r.text && r.text.trim()) {
        onAppend(r.text.trim());
      } else {
        err = 'Transcript came back empty — try speaking again.';
      }
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    } finally {
      transcribing = false;
    }
  }

  function stopStreamTracks() {
    if (stream) {
      for (const track of stream.getTracks()) {
        try {
          track.stop();
        } catch {
          /* noop */
        }
      }
      stream = null;
    }
  }

  function fmtElapsed(s: number): string {
    if (s < 60) return `${s}s`;
    return `${Math.floor(s / 60)}m ${s % 60}s`;
  }

  onDestroy(() => {
    stop();
    stopStreamTracks();
  });
</script>

{#if supported}
  <div class="voice">
    <button
      type="button"
      class="mic"
      class:recording
      class:transcribing
      onclick={recording ? stop : start}
      disabled={transcribing}
      title={recording
        ? 'Stop dictation'
        : transcribing
          ? 'Transcribing…'
          : 'Dictate (audio uploaded to Postern, transcribed by your configured AI provider)'}
      aria-label={recording ? 'Stop dictation' : 'Start dictation'}
    >
      <svg viewBox="0 0 20 20" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <rect x="7.5" y="2.5" width="5" height="10" rx="2.5" />
        <path d="M4.5 9a5.5 5.5 0 0 0 11 0" />
        <path d="M10 14.5v3" />
      </svg>
      <span class="label">
        {#if transcribing}
          Transcribing…
        {:else if recording}
          Stop ({fmtElapsed(elapsedSec)})
        {:else}
          Dictate
        {/if}
      </span>
    </button>
    {#if err}
      <span class="err">⚠ {err}</span>
    {/if}
  </div>
{/if}

<style>
  .voice {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    font-size: 0.78rem;
  }
  button.mic {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.35rem 0.75rem;
    border-radius: 0.4rem;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: var(--surface);
    color: inherit;
    font: inherit;
    font-size: 0.78rem;
    cursor: pointer;
  }
  button.mic:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 6%, var(--surface));
  }
  button.mic:disabled {
    cursor: progress;
    opacity: 0.7;
  }
  button.mic.recording {
    border-color: color-mix(in oklab, crimson 60%, transparent);
    background: color-mix(in oklab, crimson 12%, var(--surface));
    color: color-mix(in oklab, crimson 80%, currentColor);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { box-shadow: 0 0 0 0 color-mix(in oklab, crimson 30%, transparent); }
    50% { box-shadow: 0 0 0 5px color-mix(in oklab, crimson 6%, transparent); }
  }
  .label {
    font-weight: 600;
    letter-spacing: -0.01em;
    font-variant-numeric: tabular-nums;
  }
  .err {
    color: #ef4444;
    font-size: 0.76rem;
  }
</style>
