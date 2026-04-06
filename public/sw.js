// TUICommander — Service Worker
// Handles push notifications and auto-update lifecycle.
// No fetch interception or asset caching — the HTTP server owns content delivery.

// Activate immediately when installed (skip waiting for old tabs to close).
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
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
    icon: "/mobile-icon.svg",
    badge: "/mobile-icon.svg",
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
