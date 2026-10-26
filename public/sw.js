// TUICommander — Service Worker
// Handles push notifications, offline splash, and auto-update lifecycle.

const CACHE_NAME = "tuic-shell-v2";

// Pre-cache the inline offline splash on install.
self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.put(
      new Request("/_offline"),
      new Response(OFFLINE_SPLASH_HTML, {
        headers: { "Content-Type": "text/html; charset=utf-8" },
      }),
    )).then(() => self.skipWaiting()),
  );
});

// Clean old cache versions and claim clients on activate.
self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k))),
    ).then(() => self.clients.claim()),
  );
});

// --- Fetch interception (navigation only, network-first) ---

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method === "POST" && url.pathname === "/mobile/share") {
    event.respondWith((async () => {
      const form = await event.request.formData();
      const file = form.get("attachment");
      if (!file || typeof file.name !== "string") {
        return new Response("No shared file", { status: 400 });
      }
      const key = crypto.randomUUID();
      const cache = await caches.open(CACHE_NAME);
      await cache.put(`/_shared/${key}`, new Response(file, {
        headers: { "content-type": file.type || "application/octet-stream", "x-file-name": file.name },
      }));
      return new Response(null, { status: 303, headers: { location: `/mobile?shared=${key}` } });
    })());
    return;
  }
  // Keep hashed assets available across interrupted connections.
  if (event.request.method === "GET" && url.pathname.startsWith("/assets/") && url.origin === self.location.origin) {
    event.respondWith(cacheFirstAsset(event.request));
    return;
  }
  // Only intercept navigation requests (HTML page loads)
  if (event.request.mode !== "navigate") return;

  event.respondWith(
    fetchNavigation(event.request)
      .then((response) => {
        if (response.status >= 500) throw new Error(`HTTP ${response.status}`);
        return response;
      })
      .catch(() =>
        // The shell cannot work without its API. Always show the self-contained
        // connection test, even if a previous successful shell is cached.
        caches.match("/_offline").then((cached) => cached || new Response(OFFLINE_SPLASH_HTML, {
          headers: { "Content-Type": "text/html; charset=utf-8" },
        })),
      ),
  );
});

// Navigation has a short health deadline; a dropped connection must not hold
// the browser on its previous empty document until the OS TCP timeout expires.
async function fetchNavigation(request) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 5000);
  try {
    return await fetch(request, { signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}

// --- Built asset cache (hashed, immutable) ---

const MAX_CACHED_ASSETS = 200;

async function cacheFirstAsset(request) {
  const cache = await caches.open(CACHE_NAME);
  const hit = await cache.match(request);
  if (hit) return hit;
  const response = await fetch(request);
  // A pruned chunk gets the SPA fallback (HTML, 200); caching it would pin the wrong body for that URL.
  const isHtml = (response.headers?.get("content-type") || "").includes("text/html");
  if (response.ok && !isHtml) {
    await cache.put(request, response.clone());
    await trimAssets(cache);
  }
  return response;
}

// Every rebuild adds new hashed names; drop the oldest assets (keys() is in insertion order).
async function trimAssets(cache) {
  const keys = (await cache.keys()).filter((k) => new URL(k.url).pathname.startsWith("/assets/"));
  await Promise.all(keys.slice(0, Math.max(0, keys.length - MAX_CACHED_ASSETS)).map((k) => cache.delete(k)));
}

// --- Push notifications ---

self.addEventListener("push", (event) => {
  let data = {};
  if (event.data) {
    try {
      const parsed = event.data.json();
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) data = parsed;
    } catch {
      // Malformed push payload — show generic notification
    }
  }
  const title = data.title || "TUICommander";
  const url = data.url || "/mobile";
  const sessionId = /^\/mobile\/session\/([^/?#]+)\/?(?:[?#]|$)/.exec(url)?.[1];
  const chatSessionId = /(?:[?&])session=([^&#]+)/.exec(url)?.[1];
  const options = {
    body: data.body || "",
    icon: "/mobile-icon.svg?v=2",
    badge: "/mobile-icon.svg?v=2",
    data: { url },
    tag: sessionId ? `tuic-session-${sessionId}` : chatSessionId ? `tuic-acp-${chatSessionId}` : "tuic-push",
    renotify: true,
  };
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = event.notification.data?.url || "/mobile";
  event.waitUntil(
    clients.matchAll({ type: "window" }).then((windowClients) => {
      // Focus existing window and navigate to the deep link
      for (const client of windowClients) {
        if (client.url.includes("/mobile") && "focus" in client) {
          client.navigate(url);
          return client.focus();
        }
      }
      // Otherwise open a new window
      return clients.openWindow(url);
    })
  );
});

// Re-subscribe on push subscription change (edge case: browser rotates keys)
self.addEventListener("pushsubscriptionchange", (event) => {
  event.waitUntil(
    self.registration.pushManager.subscribe(event.oldSubscription?.options || {
      userVisibleOnly: true,
    }).then((sub) =>
      fetch("/api/push/subscribe", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(sub.toJSON()),
      }),
    ).catch(() => {
      // Best-effort — user will need to re-enable push manually if this fails
    }),
  );
});

// --- Inline offline splash page ---

const OFFLINE_SPLASH_HTML = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>TUICommander — Offline</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
  :root {
    --bg-primary: #1e1e1e; --bg-tertiary: #2d2d30; --bg-highlight: #37373d;
    --fg-primary: #cccccc; --fg-secondary: #a0a0a0; --border: #3e3e42;
    --accent: #59a8dd; --radius-lg: 6px;
  }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: var(--bg-primary); color: var(--fg-primary);
    display: flex; align-items: center; justify-content: center;
    min-height: 100vh; min-height: 100dvh;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
  }
  .card {
    text-align: center; max-width: 320px; padding: 32px 24px;
  }
  .icon {
    width: 48px; height: 48px; margin: 0 auto 16px;
    opacity: 0.5;
  }
  h1 { font-size: 18px; font-weight: 600; margin-bottom: 8px; }
  p { font-size: 14px; color: var(--fg-secondary); line-height: 1.5; margin-bottom: 20px; }
  .countdown { font-variant-numeric: tabular-nums; }
  button {
    background: var(--bg-tertiary); color: var(--fg-primary); border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 8px 20px; font-size: 14px;
    cursor: pointer; transition: background 0.15s;
  }
  button:hover { background: var(--bg-highlight); }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .server, .error { overflow-wrap: anywhere; }
  @keyframes pulse { 0%,100% { opacity: 0.5; } 50% { opacity: 1; } }
  .pulsing { animation: pulse 2s ease-in-out infinite; }
  @media (prefers-reduced-motion: reduce) { .pulsing { animation: none; } }
</style>
</head>
<body>
<div class="card" data-connection-landing>
  <svg class="icon pulsing" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z"/>
    <path d="M12 6v6l4 2"/>
  </svg>
  <h1>Server Unreachable</h1>
  <p class="server" id="server"></p>
  <p id="state" role="status">Testing connection…</p>
  <p class="error" id="error"></p>
  <p>Retrying in <span class="countdown" id="cd">5</span>s...</p>
  <button id="retry">Retry Now</button>
</div>
<script>
  let delay = 5;
  let remaining = delay;
  let timer;
  let testing = false;
  const cd = document.getElementById("cd");
  const btn = document.getElementById("retry");
  const state = document.getElementById("state");
  const error = document.getElementById("error");
  document.getElementById("server").textContent = location.origin;

  function tick() {
    remaining--;
    cd.textContent = remaining;
    if (remaining <= 0) attempt();
  }

  async function attempt() {
    if (testing) return;
    testing = true;
    clearInterval(timer);
    state.textContent = "Testing connection…";
    cd.textContent = "…";
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);
    try {
      const response = await fetch("/api/version", { cache: "no-store", signal: controller.signal });
      if (!response.ok) throw new Error("HTTP " + response.status);
      const version = await response.json();
      if (typeof version.version !== "string") throw new Error("Invalid server response");
      const destination = await fetch(location.href, { cache: "no-store", signal: controller.signal });
      if (!destination.ok) throw new Error("HTTP " + destination.status);
      state.textContent = "Reachable — opening the app…";
      // Keep the original deep link and query. The splash is the navigation
      // response, not another offline URL to lose the user's destination.
      location.replace(location.href);
    } catch (failure) {
      state.textContent = "Server unreachable";
      error.textContent = "Last error: " + (controller.signal.aborted ? "Connection timed out" : String(failure));
      remaining = delay;
      delay = Math.min(delay * 2, 30);
      cd.textContent = remaining;
      timer = setInterval(tick, 1000);
    } finally {
      clearTimeout(timeout);
      testing = false;
    }
  }

  btn.addEventListener("click", () => { delay = 5; attempt(); });
  window.addEventListener("online", attempt);
  window.addEventListener("pageshow", attempt);
  document.addEventListener("visibilitychange", () => { if (!document.hidden) attempt(); });
  attempt();
</script>
</body>
</html>`;
