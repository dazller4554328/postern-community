<script lang="ts">
  type ComposeMode = 'new' | 'reply' | 'reply_all' | 'forward';
  interface Props {
    composeMode: ComposeMode;
  }
  let { composeMode }: Props = $props();

  let title = $derived(
    composeMode === 'reply_all'
      ? 'Reply all'
      : composeMode === 'reply'
        ? 'Reply'
        : composeMode === 'forward'
          ? 'Forward'
          : 'New message'
  );
</script>

<header class="hero" class:compact-hero={composeMode !== 'new'}>
  <div class="hero-copy">
    <span class="eyebrow">Outbound Channel</span>
    <h1>{title}</h1>
    <p>
      Draft inside a cleaner operational console with room for encryption controls,
      attachments, and long-form mail without the cramped old layout.
    </p>
  </div>
  <div class="hero-side">
    <span class="hero-chip">PGP-ready</span>
    <span class="hero-chip">Local autocomplete</span>
    <span class="hero-chip">Attachment staging</span>
  </div>
</header>

<style>
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding: 1.3rem 1.4rem;
    margin-bottom: 1rem;
    border: 1px solid var(--border);
    border-radius: 1.3rem;
    background:
      radial-gradient(circle at top right, color-mix(in oklab, var(--accent) 12%, transparent), transparent 35%),
      linear-gradient(180deg, color-mix(in oklab, var(--surface) 88%, white 12%), var(--surface));
    box-shadow: 0 18px 42px rgba(0, 0, 0, 0.08);
  }
  .eyebrow {
    display: inline-block;
    margin-bottom: 0.55rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
    font-weight: 700;
  }
  header h1 { flex-shrink: 0; }
  .hero h1 {
    margin: 0 0 0.45rem;
    font-weight: 650;
    font-size: 2rem;
    letter-spacing: -0.03em;
  }
  .hero p {
    margin: 0;
    max-width: 46rem;
    color: var(--muted);
    line-height: 1.55;
  }
  .hero-side {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    justify-content: flex-end;
    align-content: start;
  }
  .hero-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.42rem 0.72rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--surface-2) 82%, transparent);
    border: 1px solid color-mix(in oklab, currentColor 8%, transparent);
    font-size: 0.72rem;
    font-weight: 600;
  }

  @media (max-width: 900px) {
    .hero {
      gap: 0.75rem;
      padding: 0.95rem 1rem;
      margin-bottom: 0.8rem;
      border-radius: 1rem;
      grid-template-columns: 1fr;
    }
    .hero.compact-hero {
      padding: 0.65rem 0.8rem;
      margin-bottom: 0.6rem;
      gap: 0.35rem;
      border-radius: 0.9rem;
    }
    .eyebrow {
      margin-bottom: 0.35rem;
      font-size: 0.62rem;
    }
    .hero.compact-hero .eyebrow {
      margin-bottom: 0.2rem;
      font-size: 0.58rem;
    }
    .hero h1 {
      font-size: 1.3rem;
      margin-bottom: 0.25rem;
    }
    .hero.compact-hero h1 {
      font-size: 1rem;
      margin-bottom: 0;
    }
    .hero p {
      font-size: 0.82rem;
      line-height: 1.45;
    }
    .hero.compact-hero p {
      display: none;
    }
    .hero-side {
      justify-content: flex-start;
      gap: 0.3rem;
    }
    .hero.compact-hero .hero-side {
      display: none;
    }
    .hero-chip {
      padding: 0.28rem 0.5rem;
      font-size: 0.66rem;
    }
  }
</style>
