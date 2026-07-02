// Pure SPA: `adapter-static` with `fallback: index.html` handles routing.
// `ssr = false` keeps everything client-side so dynamic routes like
// /message/[id] don't need prerender crawling.
export const ssr = false;
export const prerender = false;
