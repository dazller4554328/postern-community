# Postern marketing site (`postern.email`)

The landing site for Postern — separate from the docs (MkDocs →
`docs.postern.email`) and from the app (`web/`). Built with SvelteKit +
`adapter-static`, so it compiles to plain prerendered HTML/CSS/JS with no
runtime server.

## Stack

- SvelteKit 2 / Svelte 5 (runes)
- `@sveltejs/adapter-static` — every route prerendered, no SPA fallback
- Vanilla CSS with design tokens (`src/lib/styles/tokens.css`); no UI framework
- Fonts: Space Grotesk (display) + Inter (body), loaded with preconnect + swap

## Develop

```bash
cd www
npm install
npm run dev      # http://localhost:5174
npm run check    # svelte-check (type + a11y)
npm run build    # static output → www/build
npm run preview  # serve the production build
```

## Pages

| Route        | File                                |
| ------------ | ----------------------------------- |
| `/`          | `src/routes/+page.svelte`           |
| `/features`  | `src/routes/features/+page.svelte`  |
| `/pricing`   | `src/routes/pricing/+page.svelte`   |
| `/download`  | `src/routes/download/+page.svelte`  |

Shared copy/links live in `src/lib/site.ts` — edit URLs and feature lists
there, not inline in pages.

## Screenshots

App screenshots live in `static/img/app/`. They are the **sanitised** set
provided for marketing use (personal data removed). Swap them by replacing the
files; keep the same paths or update `site.ts` / the page that references them.

## Deploy

`npm run build` writes a fully static `build/` directory (Brotli + gzip
precompressed). Serve that directory at the `postern.email` root. It is
independent of the MkDocs docs build and of the app release build.

> **Note:** wiring the `postern.email` vhost/reverse-proxy to serve
> `www/build` is a server-side step. The post-receive release hook on the
> `postern-email` host currently skips release builds only for docs paths
> (`docs/`, `mkdocs.yml`, `README.md`, …); decide whether `www/` should
> trigger a static rebuild + publish or be handled by a separate job before
> pushing to that remote.
