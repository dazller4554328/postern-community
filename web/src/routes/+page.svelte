<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api, type Account } from '$lib/api';

  let loading = $state(true);

  onMount(async () => {
    try {
      const accounts = await api.listAccounts();
      if (accounts.length === 0) {
        goto('/setup');
      } else {
        goto('/inbox');
      }
    } catch (e) {
      console.error(e);
      loading = false;
    }
  });
</script>

<section>
  {#if loading}
    <p class="loading">Loading Postern…</p>
  {:else}
    <h1>Postern</h1>
    <p class="err">Couldn't reach the server. Is it running?</p>
  {/if}
</section>

<style>
  section {
    text-align: center;
  }
  .loading {
    opacity: 0.5;
  }
  .err {
    color: #c83333;
  }
</style>
