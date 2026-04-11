// TUICommander — Service Worker
// Handles push notifications, offline splash, and auto-update lifecycle.

const CACHE_NAME = "tuic-shell-v1";

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
  // Only intercept navigation requests (HTML page loads)
  if (event.request.mode !== "navigate") return;

  event.respondWith(
    fetch(event.request)
      .then((response) => {
        // Cache mobile.html on successful navigation for offline use
        if (response.ok) {
          const url = new URL(event.request.url);
          if (url.pathname === "/mobile" || url.pathname === "/mobile.html"
              || url.pathname.startsWith("/mobile/")) {
            const clone = response.clone();
            caches.open(CACHE_NAME).then((cache) => cache.put("/mobile.html", clone));
          }
        }
        return response;
      })
      .catch(() =>
        // Server unreachable — try cached shell, then inline offline splash
        caches.match("/mobile.html")
          .then((cached) => cached || caches.match("/_offline"))
          .then((fallback) => fallback || new Response(OFFLINE_SPLASH_HTML, {
            headers: { "Content-Type": "text/html; charset=utf-8" },
          })),
      ),
  );
});

// --- Push notifications ---

self.addEventListener("push", (event) => {
  let data = {};
  if (event.data) {
    try {
      data = event.data.json();
    } catch {
      // Malformed push payload — show generic notification
    }
  }
  const title = data.title || "TUICommander";
  const options = {
    body: data.body || "",
    icon: "/mobile-icon.svg?v=2",
    badge: "/mobile-icon.svg?v=2",
    data: { url: data.url || "/mobile" },
    tag: "tuic-push",
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
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: #0d1117; color: #c9d1d9;
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
  p { font-size: 14px; color: #8b949e; line-height: 1.5; margin-bottom: 20px; }
  .countdown { font-variant-numeric: tabular-nums; }
  button {
    background: #21262d; color: #c9d1d9; border: 1px solid #30363d;
    border-radius: 6px; padding: 8px 20px; font-size: 14px;
    cursor: pointer; transition: background 0.15s;
  }
  button:hover { background: #30363d; }
  @keyframes pulse { 0%,100% { opacity: 0.5; } 50% { opacity: 1; } }
  .pulsing { animation: pulse 2s ease-in-out infinite; }
</style>
</head>
<body>
<div class="card">
  <svg class="icon pulsing" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z"/>
    <path d="M12 6v6l4 2"/>
  </svg>
  <h1>Server Unreachable</h1>
  <p>Retrying in <span class="countdown" id="cd">5</span>s...</p>
  <button id="retry">Retry Now</button>
</div>
<script>
  let delay = 5;
  let remaining = delay;
  let timer;
  const cd = document.getElementById("cd");
  const btn = document.getElementById("retry");

  function tick() {
    remaining--;
    cd.textContent = remaining;
    if (remaining <= 0) attempt();
  }

  function attempt() {
    clearInterval(timer);
    cd.textContent = "...";
    fetch("/api/version").then((r) => {
      if (r.ok) location.replace("/mobile");
      else schedule();
    }).catch(() => schedule());
  }

  function schedule() {
    delay = Math.min(delay * 2, 30);
    remaining = delay;
    cd.textContent = remaining;
    timer = setInterval(tick, 1000);
  }

  btn.addEventListener("click", () => { delay = 5; attempt(); });
  timer = setInterval(tick, 1000);
</script>
</body>
</html>`;

