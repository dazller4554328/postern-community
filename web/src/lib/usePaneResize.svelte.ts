// Resizable-pane layout geometry for the inbox view: the sidebar /
// message-list / sender-column sizes (in px) and the pointer-drag
// handler that adjusts them within min/max clamps. Pure layout state —
// no data or API coupling — extracted from inbox/+page.svelte so the
// page component stays focused on data flow and this stays reusable.

export type ResizeTarget = 'sidebar' | 'list-x' | 'list-y' | 'from';

/** The persisted layout fields this composable owns. Other layout prefs
 *  (split orientation, density, sidebar visibility) live on the page. */
export interface SavedPaneSizes {
  sidebarWidth?: unknown;
  listWidth?: unknown;
  listHeight?: unknown;
  fromWidth?: unknown;
}

const MIN_SIDEBAR = 240;
const MAX_SIDEBAR = 480;
const MIN_LIST = 280;
const MAX_LIST = 900;
const MIN_LIST_HEIGHT = 160;
const MAX_LIST_HEIGHT = 900;
const MIN_FROM = 80;
const MAX_FROM = 420;

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

export function usePaneResize() {
  let sidebarWidth = $state(288);
  let listWidth = $state(480);
  let listHeight = $state(360);
  let fromWidth = $state(160); // sender-column width

  function startResize(target: ResizeTarget, e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const axis: 'x' | 'y' = target === 'list-y' ? 'y' : 'x';
    const start = axis === 'x' ? e.clientX : e.clientY;
    const startW =
      target === 'sidebar'
        ? sidebarWidth
        : target === 'list-x'
          ? listWidth
          : target === 'from'
            ? fromWidth
            : listHeight;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);

    const onMove = (ev: PointerEvent) => {
      const delta = (axis === 'x' ? ev.clientX : ev.clientY) - start;
      if (target === 'sidebar') {
        sidebarWidth = clamp(startW + delta, MIN_SIDEBAR, MAX_SIDEBAR);
      } else if (target === 'list-x') {
        listWidth = clamp(startW + delta, MIN_LIST, MAX_LIST);
      } else if (target === 'from') {
        fromWidth = clamp(startW + delta, MIN_FROM, MAX_FROM);
      } else {
        listHeight = clamp(startW + delta, MIN_LIST_HEIGHT, MAX_LIST_HEIGHT);
      }
    };
    const onUp = (ev: PointerEvent) => {
      el.releasePointerCapture(ev.pointerId);
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('pointercancel', onUp);
    };
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp);
    el.addEventListener('pointercancel', onUp);
  }

  /** Apply persisted sizes (from localStorage), clamping each to range. */
  function applySaved(p: SavedPaneSizes) {
    if (typeof p.sidebarWidth === 'number')
      sidebarWidth = clamp(p.sidebarWidth, MIN_SIDEBAR, MAX_SIDEBAR);
    if (typeof p.listWidth === 'number') listWidth = clamp(p.listWidth, MIN_LIST, MAX_LIST);
    if (typeof p.listHeight === 'number')
      listHeight = clamp(p.listHeight, MIN_LIST_HEIGHT, MAX_LIST_HEIGHT);
    if (typeof p.fromWidth === 'number') fromWidth = clamp(p.fromWidth, MIN_FROM, MAX_FROM);
  }

  return {
    get sidebarWidth() {
      return sidebarWidth;
    },
    get listWidth() {
      return listWidth;
    },
    get listHeight() {
      return listHeight;
    },
    get fromWidth() {
      return fromWidth;
    },
    startResize,
    applySaved
  };
}
