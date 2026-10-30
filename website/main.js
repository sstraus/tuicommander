// Leave content visible when the browser cannot observe viewport intersections.
if ("IntersectionObserver" in window) {
  const reveals = new IntersectionObserver((entries, observer) => {
    for (const entry of entries) {
      if (entry.intersectionRatio < 0.1) continue;
      entry.target.classList.add("sal-animate");
      observer.unobserve(entry.target);
    }
  }, { threshold: 0.1, rootMargin: "0% 50%" });
  document.querySelectorAll("[data-sal]").forEach((element) => reveals.observe(element));
} else {
  document.body.classList.add("sal-disabled");
}

// --- Nav: current page, mobile menu, Features dropdown ---
const path = location.pathname.replace(/index\.html$/, "");
document.querySelectorAll(".nav a, .nav-menu-list a").forEach((a) => {
  if (a.getAttribute("href") === path) a.setAttribute("aria-current", "page");
});
if (path.startsWith("/blog")) document.querySelector('.nav-links > a[href="/blog.html"]')?.setAttribute("aria-current", "page");

const toggle = document.querySelector(".nav-toggle");
const links = document.getElementById("nav-links");
toggle?.addEventListener("click", () => {
  const open = links.classList.toggle("open");
  toggle.setAttribute("aria-expanded", String(open));
});
const menu = document.querySelector(".nav-menu");
document.addEventListener("click", (e) => {
  if (menu?.open && !menu.contains(e.target) && window.innerWidth > 768) menu.open = false;
});

// --- Download buttons follow the visitor's OS ---
const ua = navigator.userAgent || "";
const platform = navigator.platform || "";
let os = "linux";
if (/Mac|iPhone|iPad/.test(platform) || /Mac OS/.test(ua)) os = "macos";
else if (/Win/.test(platform)) os = "windows";

const labels = { macos: "Download for macOS", windows: "Download for Windows", linux: "Download for Linux" };
const assetSuffix = { macos: ".dmg", windows: "x64-setup.exe", linux: "amd64.AppImage" };
const osButtons = document.querySelectorAll("[data-download]");
osButtons.forEach((b) => {
  if (b.dataset.download === "auto") b.textContent = labels[os];
});
document.querySelector(`.os-card[data-os="${os}"]`)?.classList.add("current");

// The newest versioned release (a beta while 2.0 is in beta) drives the main
// buttons; "/releases/latest" is the newest stable and drives [data-stable].
function assetFor(release, target) {
  const suffix = assetSuffix[target === "auto" ? os : target];
  return (release.assets || []).find((a) => suffix && a.name.endsWith(suffix));
}
function applyRelease(release, buttons, badges) {
  if (!release) return;
  buttons.forEach((b) => {
    const asset = assetFor(release, b.dataset.download || b.dataset.stable);
    if (asset) b.href = asset.browser_download_url;
  });
  badges.forEach((el) => {
    if (release.tag_name) el.textContent = release.tag_name;
  });
}
const api = "https://api.github.com/repos/sstraus/tuicommander/releases";
if (osButtons.length || document.querySelector(".version-badge")) {
  fetch(`${api}?per_page=20`)
    .then((r) => r.json())
    .then((releases) => {
      const newest = Array.isArray(releases)
        ? releases.find((r) => !r.draft && /^v\d/.test(r.tag_name))
        : null;
      applyRelease(newest, osButtons, document.querySelectorAll(".version-badge"));
    })
    .catch(() => {});
}
const stableButtons = document.querySelectorAll("[data-stable]");
if (stableButtons.length || document.querySelector(".stable-badge")) {
  fetch(`${api}/latest`)
    .then((r) => r.json())
    .then((release) => applyRelease(release, stableButtons, document.querySelectorAll(".stable-badge")))
    .catch(() => {});
}

// --- Install tabs and copy buttons ---
const tabs = document.querySelectorAll(".install-tab");
const panels = document.querySelectorAll(".install-panel");
function selectTab(target) {
  tabs.forEach((t) => {
    const active = t.dataset.tab === target;
    t.classList.toggle("active", active);
    t.setAttribute("aria-selected", active ? "true" : "false");
  });
  panels.forEach((p) => p.classList.toggle("active", p.dataset.panel === target));
}
if (tabs.length) selectTab(os === "windows" ? "windows" : "unix");
tabs.forEach((t) => t.addEventListener("click", () => selectTab(t.dataset.tab)));

document.querySelectorAll(".copy-btn").forEach((btn) => {
  const icon = btn.innerHTML;
  btn.addEventListener("click", () => {
    const code = btn.closest(".install-code").querySelector("code");
    navigator.clipboard.writeText(code.textContent).then(() => {
      btn.textContent = "Copied";
      setTimeout(() => (btn.innerHTML = icon), 2000);
    });
  });
});

// --- Film: a missing file leaves the poster with a note; chapters seek ---
const video = document.querySelector(".film video");
video?.querySelector("source")?.addEventListener("error", () => {
  video.removeAttribute("controls");
  video.closest(".film").querySelector(".film-missing").hidden = false;
});

document.querySelectorAll(".chapters button[data-t]").forEach((btn) => {
  btn.addEventListener("click", () => {
    if (!video) return;
    video.currentTime = Number(btn.dataset.t);
    video.play().catch(() => {});
    video.scrollIntoView({ behavior: "smooth", block: "center" });
  });
});
video?.addEventListener("timeupdate", () => {
  let current = null;
  document.querySelectorAll(".chapters button[data-t]").forEach((btn) => {
    if (Number(btn.dataset.t) <= video.currentTime) current = btn;
    btn.removeAttribute("aria-current");
  });
  current?.setAttribute("aria-current", "true");
});
