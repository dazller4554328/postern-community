<!--
  Sandboxed PDF viewer backed by Mozilla's PDF.js.
  Goals:
    * No native PDF engine → no Adobe Reader / Foxit exploit surface.
    * No PDF-JS execution → scripting is off at the library level.
    * No network side-effects from the rendered PDF → strict CSP
      (connect-src self only; no remote fetches for images, fonts,
      or tracking pixels).
    * Close-the-tab is disposal → everything lives in the tab's
      process; the canvas buffers vanish when the tab closes.

  URL: /viewer/pdf?msg=<message_id>&idx=<attachment_index>

  The fetch goes through the same /api/body/:id/attachment/:index
  endpoint used by the Download button, so auth cookies apply and no
  new server route is needed.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';

  let messageId = $state<number | null>(null);
  let attachmentIndex = $state<number | null>(null);
  let filename = $state<string | null>(null);

  let pdfDoc: unknown = null;
  let pageCount = $state(0);
  let currentPage = $state(1);
  let zoom = $state(1.0);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Canvas refs — one per page, mounted on demand. Must be $state so
  // bind:this registers the assignment as reactive; a plain let would
  // mean IntersectionObserver setup can't see the mounted element.
  let canvasContainer = $state<HTMLDivElement | null>(null);
  // Guard: render the CURRENT page on load; other pages render when
  // scrolled into view via IntersectionObserver so a 200-page PDF
  // doesn't eat all the user's RAM up front.
  let renderObserver: IntersectionObserver | null = null;
  const renderedPages = new Set<number>();

  onMount(async () => {
    const params = $page.url.searchParams;
    const msg = Number(params.get('msg'));
    const idx = Number(params.get('idx'));
    filename = params.get('name');
    if (!Number.isFinite(msg) || msg <= 0 || !Number.isFinite(idx) || idx < 0) {
      error = 'Missing or invalid msg / idx query params.';
      loading = false;
      return;
    }
    messageId = msg;
    attachmentIndex = idx;

    try {
      // Dynamic import — PDF.js is ~1.5 MB, don't bloat the main bundle
      // for users who never hit the viewer.
      const pdfjs = await import('pdfjs-dist');
      // Worker is required; Vite handles bundling this as its own chunk.
      // Using ?url to get the worker's final path; new Worker(url, {type:'module'})
      // is what PDF.js expects for module workers.
      const workerUrl = (
        await import('pdfjs-dist/build/pdf.worker.min.mjs?url')
      ).default;
      pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

      // When ?render=1 is present, the attachment isn't a PDF —
      // it's an Office / OpenDocument file we need the viewer
      // sandbox to convert first. The render-pdf endpoint speaks
      // to the sibling container over a Unix socket and streams
      // back PDF bytes (or 5xx if the sandbox isn't running).
      const shouldRender = params.get('render') === '1';
      const fetchUrl = shouldRender
        ? `/api/messages/${msg}/attachment/${idx}/render-pdf`
        : `/api/messages/${msg}/attachment/${idx}?mode=inline`;

      const res = await fetch(fetchUrl, { credentials: 'same-origin' });
      if (!res.ok) {
        // Our API returns JSON {"error":"..."} for 4xx/5xx — try to
        // surface that directly instead of a cryptic status line.
        let msg = `${res.status} ${res.statusText}`;
        try {
          const body = await res.json();
          if (body?.error) msg = body.error;
        } catch { /* not JSON — use the status line */ }
        throw new Error(msg);
      }
      const buf = await res.arrayBuffer();

      // Diagnose the common "it's not actually a PDF we got" failure
      // modes BEFORE handing it to PDF.js — PDF.js's own errors are
      // opaque ("Invalid PDF structure") and don't tell us whether we
      // got base64-still-encoded bytes, an HTML error page, or just a
      // corrupt file.
      const head = new Uint8Array(buf.slice(0, 8));
      const asText = new TextDecoder('latin1').decode(head);
      if (!asText.startsWith('%PDF-')) {
        let hint = '';
        if (asText.startsWith('JVBERi0')) {
          hint = ' — server handed us base64-encoded bytes instead of the decoded PDF. This is a server bug; please report it.';
        } else if (/^<!doctype|^<html/i.test(asText)) {
          hint = ' — server returned an HTML page (probably an error page) where a PDF was expected.';
        } else if (asText.startsWith('PK\x03\x04')) {
          hint = ' — this file is actually a ZIP (.docx, .xlsx, .odt and similar all look like this). Download to open it in the right app.';
        } else {
          const hex = Array.from(head).map((b) => b.toString(16).padStart(2, '0')).join(' ');
          hint = ` — first 8 bytes are: ${hex}. Expected "25 50 44 46 2d" ("%PDF-").`;
        }
        throw new Error(`Not a valid PDF${hint}`);
      }

      // isEvalSupported + enableXfa + disableAutoFetch: close off the
      // legacy PDF JavaScript / XFA forms execution paths. We want a
      // static render, not a scripted document.
      const loadingTask = pdfjs.getDocument({
        data: buf,
        isEvalSupported: false,
        disableAutoFetch: true,
        enableXfa: false
      });
      pdfDoc = await loadingTask.promise;
      // @ts-expect-error we know pdfDoc has .numPages after load
      pageCount = pdfDoc.numPages;
      loading = false;

      // Render first page immediately; set up lazy render for the rest.
      await renderPage(1);
      setupLazyRender();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      loading = false;
    }
  });

  onDestroy(() => {
    renderObserver?.disconnect();
    pdfDoc = null;
  });

  async function renderPage(n: number) {
    if (!pdfDoc || renderedPages.has(n)) return;
    // @ts-expect-error dynamic import
    const page = await pdfDoc.getPage(n);
    const viewport = page.getViewport({ scale: zoom * (window.devicePixelRatio || 1) });
    const canvas = document.getElementById(`pdf-page-${n}`) as HTMLCanvasElement | null;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    canvas.width = viewport.width;
    canvas.height = viewport.height;
    canvas.style.width = `${viewport.width / (window.devicePixelRatio || 1)}px`;
    canvas.style.height = `${viewport.height / (window.devicePixelRatio || 1)}px`;
    await page.render({ canvasContext: ctx, viewport, canvas }).promise;
    renderedPages.add(n);
  }

  function setupLazyRender() {
    // Observe page placeholders; render when they scroll into the
    // viewport. Saves RAM on large docs.
    if (!canvasContainer) return;
    renderObserver = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        const n = Number((e.target as HTMLElement).dataset.page);
        if (n && !renderedPages.has(n)) void renderPage(n);
      }
    }, { rootMargin: '400px' });
    const placeholders = canvasContainer.querySelectorAll('.pdf-page-slot');
    placeholders.forEach((el) => renderObserver!.observe(el));
  }

  function zoomIn() { zoom = Math.min(4, zoom + 0.25); rerenderAll(); }
  function zoomOut() { zoom = Math.max(0.25, zoom - 0.25); rerenderAll(); }
  function resetZoom() { zoom = 1; rerenderAll(); }

  function rerenderAll() {
    renderedPages.clear();
    if (!pdfDoc) return;
    for (let i = 1; i <= pageCount; i++) {
      void renderPage(i);
    }
  }

  function goToPage(n: number) {
    const clamped = Math.max(1, Math.min(pageCount, Math.round(n)));
    currentPage = clamped;
    const el = document.getElementById(`pdf-page-${clamped}`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  async function downloadOriginal() {
    if (messageId == null || attachmentIndex == null) return;
    const a = document.createElement('a');
    a.href = `/api/messages/${messageId}/attachment/${attachmentIndex}?mode=download`;
    if (filename) a.download = filename;
    a.click();
  }
</script>

<svelte:head>
  <title>{filename ?? 'PDF'} — Postern viewer</title>
  <!--
    Strict CSP applied at the document level. Meta CSP doesn't
    enforce frame-ancestors (that needs a response header), so the
    Rust server sets X-Frame-Options: SAMEORIGIN on static assets.
    Everything else the viewer depends on is here:
      - connect-src 'self'  -> fetch the PDF from our API only
      - wasm-unsafe-eval    -> PDF.js uses WebAssembly
      - worker-src blob:    -> Vite serves the worker from a bundled URL
      - img/font-src data:  -> PDF.js uses data URIs for embedded fonts
      - object-src 'none'   -> no <object>/<embed> anywhere
  -->
  <meta
    http-equiv="content-security-policy"
    content="default-src 'self';
             connect-src 'self';
             script-src 'self' 'wasm-unsafe-eval' 'unsafe-inline';
             worker-src 'self' blob:;
             style-src 'self' 'unsafe-inline';
             img-src 'self' data: blob:;
             font-src 'self' data:;
             object-src 'none';
             base-uri 'self';
             form-action 'none';
             frame-ancestors 'self';"
  />
</svelte:head>

<main class="viewer">
  <header class="toolbar">
    <div class="title">
      <strong>{filename ?? 'Attachment'}</strong>
      {#if pageCount > 0}
        <span class="muted">page {currentPage} / {pageCount}</span>
      {/if}
    </div>
    <div class="controls">
      <button type="button" onclick={() => goToPage(currentPage - 1)} disabled={loading || currentPage <= 1}>◂</button>
      <input
        type="number"
        min="1"
        max={pageCount}
        value={currentPage}
        onchange={(e) => goToPage(Number((e.currentTarget as HTMLInputElement).value))}
        aria-label="Go to page"
      />
      <button type="button" onclick={() => goToPage(currentPage + 1)} disabled={loading || currentPage >= pageCount}>▸</button>
      <span class="sep"></span>
      <button type="button" onclick={zoomOut} disabled={loading}>−</button>
      <button type="button" onclick={resetZoom} disabled={loading} title="Reset zoom">{Math.round(zoom * 100)}%</button>
      <button type="button" onclick={zoomIn} disabled={loading}>+</button>
      <span class="sep"></span>
      <button type="button" onclick={downloadOriginal} disabled={loading} title="Download the original file">⤓</button>
    </div>
  </header>

  {#if error}
    <div class="placeholder err">
      <p><strong>Couldn't render this PDF.</strong></p>
      <p class="muted">{error}</p>
      <p>
        <button type="button" onclick={downloadOriginal}>Download the file</button> and open it
        in a dedicated PDF reader instead.
      </p>
    </div>
  {:else if loading}
    <div class="placeholder">
      <p>Loading…</p>
    </div>
  {:else}
    <div
      class="pages"
      bind:this={canvasContainer}
      onscroll={(e) => {
        // Track which page the user is currently looking at so the
        // page-indicator in the toolbar stays accurate while scrolling.
        const container = e.currentTarget as HTMLElement;
        const mid = container.scrollTop + container.clientHeight / 2;
        let best = currentPage;
        for (let i = 1; i <= pageCount; i++) {
          const slot = document.getElementById(`pdf-page-${i}`)?.parentElement;
          if (!slot) continue;
          const top = slot.offsetTop;
          const bottom = top + slot.offsetHeight;
          if (mid >= top && mid < bottom) { best = i; break; }
        }
        if (best !== currentPage) currentPage = best;
      }}
    >
      {#each Array(pageCount) as _, i (i)}
        {@const n = i + 1}
        <div class="pdf-page-slot" data-page={n}>
          <canvas id={`pdf-page-${n}`}></canvas>
        </div>
      {/each}
    </div>
  {/if}
</main>

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    height: 100%;
    background: #2b2b2e;
    color: #f5f5f5;
    font-family: system-ui, sans-serif;
  }

  .viewer {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 1rem;
    background: #1e1e21;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    flex-shrink: 0;
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    overflow: hidden;
  }
  .title strong {
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    opacity: 0.55;
    font-size: 0.75rem;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-shrink: 0;
  }
  .controls button {
    background: rgba(255, 255, 255, 0.08);
    color: inherit;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    padding: 0.35rem 0.6rem;
    cursor: pointer;
    font-size: 0.85rem;
    line-height: 1;
    min-width: 2rem;
  }
  .controls button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.16);
  }
  .controls button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .controls input {
    width: 3rem;
    text-align: center;
    background: rgba(255, 255, 255, 0.06);
    color: inherit;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    padding: 0.28rem 0.3rem;
    font-size: 0.85rem;
  }
  .controls .sep {
    width: 1px;
    height: 18px;
    background: rgba(255, 255, 255, 0.12);
    margin: 0 0.15rem;
  }

  .pages {
    flex: 1;
    overflow: auto;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
  }
  .pdf-page-slot {
    /* Give unrendered pages an approximate height so the scroll
       position doesn't jump around as pages render in. A4 aspect is
       a decent default; real size snaps in on render. */
    min-height: 40vh;
    background: #fff;
    color: #000;
    border-radius: 4px;
    overflow: hidden;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.35);
  }
  .pdf-page-slot canvas {
    display: block;
  }

  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 0.4rem;
    padding: 2rem;
  }
  .placeholder.err {
    color: #fca5a5;
  }
  .placeholder button {
    background: rgba(255, 255, 255, 0.12);
    color: inherit;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    padding: 0.45rem 0.9rem;
    cursor: pointer;
    font: inherit;
  }
</style>
