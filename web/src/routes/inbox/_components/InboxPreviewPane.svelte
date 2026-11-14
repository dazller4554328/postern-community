<script lang="ts">
  import MessageBody from '$lib/components/MessageBody.svelte';

  type SplitOrient = 'vertical' | 'horizontal';

  interface Props {
    selectedId: number | null;
    splitOrient: SplitOrient;
    isMobile: boolean;
    activeFilterLabel: string;
    onResizeStart: (target: 'list-x' | 'list-y', e: PointerEvent) => void;
    onClose: () => void;
  }
  let {
    selectedId,
    splitOrient,
    isMobile,
    activeFilterLabel,
    onResizeStart,
    onClose,
  }: Props = $props();
</script>

{#if splitOrient === 'vertical'}
  <div
    class="resizer resizer-v resizer-inner"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize preview"
    onpointerdown={(e) => onResizeStart('list-x', e)}
  ></div>
{:else}
  <div
    class="resizer resizer-h resizer-inner-h"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize preview"
    onpointerdown={(e) => onResizeStart('list-y', e)}
  ></div>
{/if}

<section class="preview-pane">
  <header class="preview-header">
    <button class="close" onclick={onClose} title="Close preview (Esc)">{isMobile ? '←' : '×'}</button>
    {#if isMobile}
      <div class="preview-context">
        <span class="preview-kicker">Message</span>
        <span class="preview-title">{activeFilterLabel}</span>
      </div>
    {/if}
  </header>
  {#if selectedId !== null}
    {#key selectedId}
      <MessageBody messageId={selectedId} variant="preview" />
    {/key}
  {/if}
</section>
