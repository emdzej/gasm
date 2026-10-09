// A service worker that makes the pages it controls cross-origin isolated (COOP + COEP on
// every response), for hosts that can't send those headers themselves (GitHub Pages).
// Isolation is what browsers require for SharedArrayBuffer, so games built with threads
// get worker threads (runners/web/lib/threads.js). Registered by isolate.js, scope: its
// own folder. Requests are passed through unchanged otherwise.
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
self.addEventListener('fetch', (e) => {
  const req = e.request;
  // a cache mode the fetch below can't repeat (DevTools' "disable cache" on a navigation)
  if (req.cache === 'only-if-cached' && req.mode !== 'same-origin') return;
  e.respondWith(fetch(req).then((res) => {
    if (res.status === 0) return res;   // opaque: nothing to add to
    const headers = new Headers(res.headers);
    headers.set('Cross-Origin-Opener-Policy', 'same-origin');
    headers.set('Cross-Origin-Embedder-Policy', 'require-corp');
    if (new URL(req.url).origin === self.location.origin) headers.set('Cross-Origin-Resource-Policy', 'same-origin');
    return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
  }));
});
