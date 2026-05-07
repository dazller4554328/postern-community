<script lang="ts">
  import { page } from '$app/stores';
  import MessageBody from '$lib/components/MessageBody.svelte';

  let id = $derived(Number($page.params.id));
</script>

<article>
  <a class="back" href="/inbox">← Inbox</a>
  <MessageBody messageId={id} variant="full" />
</article>

<style>
  article {
    width: 100%;
    max-width: 54rem;
    padding: 1rem 2rem;
    box-sizing: border-box;
  }
  .back {
    display: inline-block;
    margin-bottom: 0.5rem;
    color: inherit;
    opacity: 0.55;
    text-decoration: none;
    font-size: 0.85rem;
  }

  @media (max-width: 900px) {
    article {
      max-width: none;
      padding: 0.45rem 0.45rem 0.75rem;
    }
    .back {
      margin-bottom: 0.2rem;
      padding: 0.35rem 0.4rem;
      font-size: 0.78rem;
    }
  }
</style>
