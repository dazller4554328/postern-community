// Svelte `use:` action for touch-swipe on list rows. BlueMail-style:
// short swipe reveals a coloured action (archive / delete / star /
// mark-read), release past the threshold commits the action.
//
// Call site:
//   <div use:swipe={{ onLeft: archive, onRight: toggleRead }}>...</div>
//
// Fires at most one callback per gesture. Pointer events so we get a
// single handler for touch + mouse + pen without vendor prefixes.

export interface SwipeOptions {
  /** Fired when the element is swiped left past the threshold. */
  onLeft?: () => void;
  /** Fired when the element is swiped right past the threshold. */
  onRight?: () => void;
  /** Pixels the user must drag before a callback fires. Default 72. */
  threshold?: number;
  /** Cap dragged offset so rubber-banding doesn't reveal the whole row. */
  maxOffset?: number;
}

export function swipe(node: HTMLElement, opts: SwipeOptions) {
  let options = { threshold: 72, maxOffset: 160, ...opts };
  let startX = 0;
  let startY = 0;
  let dragging = false;
  let committed = false;
  let offset = 0;

  function onPointerDown(e: PointerEvent) {
    // Ignore the right mouse button and multi-touch beyond the first.
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    startX = e.clientX;
    startY = e.clientY;
    dragging = true;
    committed = false;
    offset = 0;
    node.style.transition = 'none';
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    // Vertical gesture? User is scrolling — abort horizontal tracking.
    if (Math.abs(dy) > Math.abs(dx) && Math.abs(dy) > 8) {
      dragging = false;
      node.style.transform = '';
      return;
    }
    if (Math.abs(dx) < 4) return;
    // Lock: once we know it's a horizontal gesture, capture the pointer
    // so subsequent moves come here even if the finger leaves the row.
    if (Math.abs(dx) >= 4 && !node.hasPointerCapture(e.pointerId)) {
      node.setPointerCapture(e.pointerId);
    }
    e.preventDefault();
    const clamped = Math.max(-options.maxOffset, Math.min(options.maxOffset, dx));
    offset = clamped;
    node.style.transform = `translateX(${clamped}px)`;
    node.dataset.swipeDir = clamped < 0 ? 'left' : clamped > 0 ? 'right' : '';
  }

  function onPointerUp(_e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    node.style.transition = 'transform 180ms ease';
    const hit = Math.abs(offset) >= options.threshold;
    if (hit && !committed) {
      committed = true;
      if (offset < 0) options.onLeft?.();
      else if (offset > 0) options.onRight?.();
    }
    node.style.transform = '';
    node.dataset.swipeDir = '';
  }

  node.addEventListener('pointerdown', onPointerDown);
  node.addEventListener('pointermove', onPointerMove);
  node.addEventListener('pointerup', onPointerUp);
  node.addEventListener('pointercancel', onPointerUp);

  return {
    update(next: SwipeOptions) {
      options = { threshold: 72, maxOffset: 160, ...next };
    },
    destroy() {
      node.removeEventListener('pointerdown', onPointerDown);
      node.removeEventListener('pointermove', onPointerMove);
      node.removeEventListener('pointerup', onPointerUp);
      node.removeEventListener('pointercancel', onPointerUp);
    }
  };
}
