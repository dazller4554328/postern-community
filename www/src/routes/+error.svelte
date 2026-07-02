<script lang="ts">
  import { page } from '$app/stores';
  import Logo from '$lib/components/Logo.svelte';
  import Icon from '$lib/components/Icon.svelte';

  let status = $derived($page.status);
  let message = $derived($page.error?.message ?? 'Page not found');
</script>

<svelte:head>
  <title>{status} · Postern</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<section class="err shell">
  <Logo size={40} wordmark={false} />
  <p class="code">{status}</p>
  <h1>{status === 404 ? 'Nothing behind this door.' : 'Something went wrong.'}</h1>
  <p class="lead">
    {status === 404 ? 'That page doesn’t exist — but your inbox is still safe at home.' : message}
  </p>
  <div class="btns">
    <a class="btn btn--primary btn--lg" href="/">Back to home <Icon name="arrow" size={18} /></a>
    <a class="btn btn--ghost btn--lg" href="/features">Browse features</a>
  </div>
</section>

<style>
  .err {
    min-height: 64vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding-block: 5rem;
  }
  .code {
    font-family: var(--font-mono);
    color: var(--accent);
    letter-spacing: 0.2em;
    margin: 1.4rem 0 0.5rem;
  }
  h1 {
    font-size: var(--text-2xl);
    margin-bottom: 0.8rem;
  }
  .lead {
    margin-inline: auto;
  }
  .btns {
    display: flex;
    flex-wrap: wrap;
    gap: 0.8rem;
    justify-content: center;
    margin-top: 2rem;
  }
</style>
