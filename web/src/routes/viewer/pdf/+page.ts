// PDF.js is a client-only render pipeline — WASM, workers, canvas.
// Turn SSR off so SvelteKit doesn't try to evaluate it on the server.
// Prerender stays on (adapter-static): the HTML shell is static, the
// viewer code + query-param logic boots on the client.
export const ssr = false;
export const prerender = true;
