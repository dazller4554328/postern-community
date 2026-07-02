import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    // Fully static marketing site — every route is prerendered to HTML.
    // No SPA fallback: a missing route is a build error, not a client 404.
    adapter: adapter({
      pages: 'build',
      assets: 'build',
      // Client-rendered shell served (with a 404 status) for any unknown
      // path, so deep links and typos land on a branded error page.
      fallback: '404.html',
      precompress: false,
      strict: true
    }),
    alias: {
      $lib: 'src/lib'
    },
    // Hash-based CSP emitted as a <meta> tag in every prerendered page.
    // SvelteKit hashes its own inline bootstrap scripts automatically; the
    // single manual hash below is for the no-flash theme script in app.html.
    csp: {
      mode: 'hash',
      directives: {
        'default-src': ['self'],
        'script-src': ['self', "'sha256-QjY5o5znbX37j9DouRMMIas3RcqnJkLIghPtpyVkd/Q='"],
        'style-src': ['self', 'unsafe-inline', 'https://fonts.googleapis.com'],
        'font-src': ['self', 'https://fonts.gstatic.com'],
        'img-src': ['self', 'data:'],
        'connect-src': ['self'],
        'base-uri': ['self'],
        'form-action': ['self'],
        'frame-ancestors': ['none'],
        'object-src': ['none'],
        'upgrade-insecure-requests': true
      }
    }
  }
};

export default config;
