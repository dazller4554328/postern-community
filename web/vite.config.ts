import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/img-proxy': 'http://127.0.0.1:8080',
      '/health': 'http://127.0.0.1:8080',
      '/version': 'http://127.0.0.1:8080'
    }
  },
  test: {
    // Pure-logic unit tests live next to the code they cover
    // (src/lib/**/*.test.ts). Fast and DOM-free; component/E2E flows
    // are handled separately. Excludes the Playwright e2e dir.
    environment: 'node',
    include: ['src/**/*.{test,spec}.{js,ts}'],
    exclude: ['e2e/**', 'node_modules/**']
  }
});
