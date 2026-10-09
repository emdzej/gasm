// Cross-origin isolation for the player where the host can't send COOP/COEP (GitHub Pages):
// isolate-sw.js adds them, so the page has to load once more under the service worker.
// Resolves when the page is isolated (or can't be: no service workers, an insecure page);
// never resolves while it reloads. Only pages with <meta name="gasm-isolate"> use it.
export async function isolate(swUrl = new URL('./isolate-sw.js', import.meta.url)) {
  if (globalThis.crossOriginIsolated || !window.isSecureContext || !('serviceWorker' in navigator)) return;
  if (!document.querySelector('meta[name="gasm-isolate"]')) return;
  try {
    await navigator.serviceWorker.register(swUrl, { scope: new URL('./', swUrl).pathname });
    await navigator.serviceWorker.ready;
  } catch (e) {
    console.warn(`isolation: ${e.message}; games run without threads`);
    return;
  }
  // once per session: a page that stays unisolated (a browser that ignores the headers)
  // must not reload forever
  if (sessionStorage.getItem('gasm.isolate') === location.href) return;
  sessionStorage.setItem('gasm.isolate', location.href);
  location.reload();
  await new Promise(() => {});
}
