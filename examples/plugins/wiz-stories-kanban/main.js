/**
 * Stories Kanban Plugin — Kanban board for file-stories
 *
 * Renders stories as cards across status columns with drag-and-drop.
 * Drop triggers frontmatter + filename update. Changes accumulate in
 * a pending log; "Apply to Claude" sends them to the active PTY.
 *
 * Capabilities: fs:read, fs:list, fs:watch, fs:write, fs:rename,
 *               ui:panel, ui:markdown, pty:write
 */

const PLUGIN_ID = "wiz-stories-kanban";
const SECTION_ID = "kanban";
const STORIES_DIR = "stories";

const VIEWS = ["stories", "plans", "reviews"];
const VIEW_LABELS = { stories: "Stories", plans: "Plans", reviews: "Reviews" };
const VIEW_DIRS = { stories: "stories", plans: "plans", reviews: "reviews" };

const STATUSES = ["pending", "ready", "in_progress", "blocked", "complete", "wontfix"];

const STATUS_LABELS = {
  pending: "Pending",
  ready: "Ready",
  in_progress: "In Progress",
  blocked: "Blocked",
  complete: "Complete",
  wontfix: "Won\u2019t Fix",
};

// Plans use a flat 3-column layout: raw statuses are collapsed into groups.
const PLAN_COLUMNS = ["planning", "active", "done"];
const PLAN_COLUMN_LABELS = {
  planning: "Planning",
  active: "In Progress",
  done: "Done",
};
const PLAN_STATUS_TO_COLUMN = {
  draft: "planning",
  validated: "planning",
  in_progress: "active",
  parked: "done",
  completed: "done",
  rejected: "done",
};
const PLAN_FALLBACK_COLUMN = "planning";

// Reviews: 2 columns based on frontmatter `status` (open | triaged).
const REVIEW_COLUMNS = ["open", "triaged"];
const REVIEW_COLUMN_LABELS = { open: "Open", triaged: "Triaged" };
const REVIEW_FALLBACK_COLUMN = "open";

const ICON_KANBAN = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M1.75 0h12.5C15.216 0 16 .784 16 1.75v12.5A1.75 1.75 0 0 1 14.25 16H1.75A1.75 1.75 0 0 1 0 14.25V1.75C0 .784.784 0 1.75 0M1.5 1.75v12.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V1.75a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25M5.25 2a.75.75 0 0 1 .75.75v7.5a.75.75 0 0 1-1.5 0v-7.5A.75.75 0 0 1 5.25 2m5.5 0a.75.75 0 0 1 .75.75v4.5a.75.75 0 0 1-1.5 0v-4.5a.75.75 0 0 1 .75-.75"/></svg>`;

// ── Plugin state ────────────────────────────────────────────────────────

let hostRef = null;
let panelHandle = null;
let watchDisposable = null;
let storiesDir = null;
let repoRoot = null;
let stories = [];
let listItems = []; // { filename, displayName } for plans/reviews views
let view = "stories";
let pendingChanges = []; // { storyId, title, oldStatus, newStatus }

// ── Filename parsing ────────────────────────────────────────────────────

/**
 * Parse a story filename into its components.
 * Format: {seq}-{hash}-{status}-{priority}-{title-slug}.md
 */
function parseFilename(filename) {
  const m = filename.match(/^(\d+)-([a-f0-9]+)-([\w]+)-(P[0-3])-(.+)\.md$/);
  if (!m) return null;
  return {
    seq: parseInt(m[1], 10),
    hash: m[2],
    status: m[3],
    priority: m[4],
    slug: m[5],
  };
}

/**
 * Parse YAML frontmatter from story content.
 */
function parseFrontmatter(content) {
  const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
  if (!fmMatch) return null;
  const block = fmMatch[1];

  const get = (key) => {
    const m = block.match(new RegExp(`^${key}:\\s*"?(.+?)"?$`, "m"));
    return m ? m[1].trim() : null;
  };

  const getArray = (key) => {
    const m = block.match(new RegExp(`^${key}:\\s*\\[(.*)\\]`, "m"));
    if (!m) return [];
    return m[1]
      .split(",")
      .map((s) => s.trim().replace(/^"/, "").replace(/"$/, ""))
      .filter(Boolean);
  };

  return {
    id: get("id"),
    title: get("title"),
    status: get("status"),
    priority: get("priority"),
    created: get("created"),
    updated: get("updated"),
    dependencies: getArray("dependencies"),
  };
}

function hasWorkLog(content) {
  const wlMatch = content.match(/## Work Log\n([\s\S]*)/);
  if (!wlMatch) return false;
  return /^### /m.test(wlMatch[1]);
}

// ── Data loading ────────────────────────────────────────────────────────

async function loadStories() {
  if (!hostRef || !storiesDir) return [];

  let files;
  try {
    files = await hostRef.listDirectory(storiesDir, "*.md");
  } catch {
    return [];
  }

  const BATCH_SIZE = 20;
  const results = [];

  for (let i = 0; i < files.length; i += BATCH_SIZE) {
    const batch = files.slice(i, i + BATCH_SIZE);
    const batchResults = await Promise.all(
      batch.map(async (filename) => {
        const parsed = parseFilename(filename);
        if (!parsed) return null;

        try {
          const content = await hostRef.readFile(`${storiesDir}/${filename}`);
          const fm = parseFrontmatter(content);
          if (!fm) return null;

          return {
            id: fm.id || `${parsed.seq}-${parsed.hash}`,
            seq: parsed.seq,
            title: fm.title || parsed.slug.replace(/-/g, " "),
            status: fm.status || parsed.status,
            priority: fm.priority || parsed.priority,
            filename,
            hasWorkLog: hasWorkLog(content),
            created: fm.created,
            updated: fm.updated,
            dependencies: fm.dependencies || [],
          };
        } catch {
          return null;
        }
      }),
    );
    results.push(...batchResults);
  }

  return results.filter(Boolean).sort((a, b) => a.seq - b.seq);
}

// ── HTML rendering ──────────────────────────────────────────────────────

function esc(str) {
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderCard(story) {
  const worklogBadge = story.hasWorkLog
    ? `<span class="worklog-badge" title="Has work log">&#10003;</span>`
    : "";
  const depBadge = story.dependencies.length > 0
    ? `<span class="dep-badge" title="Has dependencies">&#128279;</span>`
    : "";

  return `<div class="card"
    data-story-id="${esc(story.id)}"
    data-filename="${esc(story.filename)}"
    data-status="${esc(story.status)}"
    data-title="${esc(story.title)}">
    <div class="card-header">
      <span class="card-id">${esc(story.id)}</span>
      ${worklogBadge}${depBadge}
    </div>
    <div class="card-title">${esc(story.title)}</div>
  </div>`;
}

function renderPendingChanges() {
  if (pendingChanges.length === 0) return "";

  const items = pendingChanges.map((c, i) =>
    `<div class="change-item">
      <span class="change-desc">${esc(c.storyId)}: ${esc(STATUS_LABELS[c.oldStatus])} \u2192 ${esc(STATUS_LABELS[c.newStatus])}</span>
      <button class="change-undo" data-idx="${i}" title="Undo">\u2715</button>
    </div>`
  ).join("");

  return `<div class="pending-bar">
    <div class="pending-header">
      <span class="pending-count">${pendingChanges.length} pending change${pendingChanges.length > 1 ? "s" : ""}</span>
      <div class="pending-actions">
        <button class="primary" id="btn-apply">Apply to Claude</button>
        <button id="btn-discard">Discard all</button>
      </div>
    </div>
    <div class="pending-list">${items}</div>
  </div>`;
}

function renderList(items, filters) {
  const { search } = filters;
  const searchLower = (search || "").toLowerCase();
  const filtered = items.filter((it) =>
    !searchLower || it.displayName.toLowerCase().includes(searchLower) || it.filename.toLowerCase().includes(searchLower)
  );

  const rows = filtered.length === 0
    ? `<div class="list-empty">No ${esc(VIEW_LABELS[view].toLowerCase())} found</div>`
    : filtered.map((it) =>
        `<div class="list-item" data-filename="${esc(it.filename)}">
          <span class="list-name">${esc(it.displayName)}</span>
        </div>`
      ).join("");

  return `<!DOCTYPE html>
<html>
<head>
<style>
  body { overflow: hidden; height: 100vh; display: flex; flex-direction: column; }
  .search-input { flex: 1; max-width: 260px; }
  .view-switcher {
    display: inline-flex; gap: 2px; margin-left: 8px;
    border: 1px solid var(--border, #3e3e42); border-radius: 4px; overflow: hidden;
  }
  .view-btn {
    padding: 3px 10px; font-size: 11px; background: transparent; border: none;
    color: var(--fg-secondary, #a0a0a0); cursor: pointer;
  }
  .view-btn:hover { background: var(--bg-tertiary, #2d2d30); }
  .view-btn.active { background: var(--accent, #59a8dd); color: var(--text-on-accent, #fff); }
  .list-view { flex: 1; overflow-y: auto; padding: 8px 12px; }
  .list-item {
    display: flex; align-items: center; gap: 8px; padding: 6px 10px;
    border-radius: 4px; cursor: pointer; font-size: 12px;
    color: var(--fg-primary, #e0e0e0);
  }
  .list-item:hover { background: var(--bg-tertiary, #2d2d30); }
  .list-item .list-name { flex: 1; }
  .list-empty {
    padding: 24px 12px; text-align: center; font-size: 12px;
    color: var(--fg-muted, #9aa1a9); font-style: italic;
  }
</style>
</head>
<body>
  <div class="filter-bar">
    <input type="search" class="search-input" placeholder="Search ${esc(view)}..." value="${esc(search || "")}">
    ${renderViewSwitcher(view)}
  </div>
  <div class="list-view">${rows}</div>

<script>
  document.querySelectorAll(".view-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      window.parent.postMessage({ type: "view-change", view: btn.dataset.view }, "*");
    });
  });

  const searchInput = document.querySelector(".search-input");
  searchInput.addEventListener("input", () => {
    window.parent.postMessage({ type: "filter-change", search: searchInput.value }, "*");
  });

  document.querySelectorAll(".list-item").forEach((item) => {
    item.addEventListener("click", () => {
      window.parent.postMessage({ type: "open-list-item", filename: item.dataset.filename }, "*");
    });
  });
</script>
</body>
</html>`;
}

function renderViewSwitcher(active) {
  return `<div class="view-switcher">${
    VIEWS.map((v) =>
      `<button class="view-btn${v === active ? " active" : ""}" data-view="${v}">${VIEW_LABELS[v]}</button>`
    ).join("")
  }</div>`;
}

function renderBoard(storyList, filters) {
  const { search } = filters;
  const searchLower = (search || "").toLowerCase();

  const filtered = storyList.filter((s) => {
    if (searchLower && !s.title.toLowerCase().includes(searchLower) && !s.id.toLowerCase().includes(searchLower)) {
      return false;
    }
    return true;
  });

  const byStatus = {};
  for (const st of STATUSES) byStatus[st] = [];
  for (const s of filtered) {
    if (byStatus[s.status]) byStatus[s.status].push(s);
  }

  const columns = STATUSES.map((st) => {
    const cards = byStatus[st].map(renderCard).join("");
    const count = byStatus[st].length;
    const placeholder = count === 0 ? `<div class="empty-col">No stories</div>` : "";
    return `<div class="column" data-status="${st}">
      <div class="col-header ${st}">
        <span class="col-label">${STATUS_LABELS[st]}</span>
        <span class="col-count">${count}</span>
      </div>
      <div class="col-body">${cards}${placeholder}</div>
    </div>`;
  }).join("");

  return `<!DOCTYPE html>
<html>
<head>
<style>
  /* Plugin-specific — base styles inherited from TUICommander */
  body {
    overflow: hidden;
    height: 100vh;
    display: flex;
    flex-direction: column;
  }

  /* ── Filter bar ── */
  .search-input {
    flex: 1;
    max-width: 260px;
  }
  .view-switcher {
    display: inline-flex;
    gap: 2px;
    margin-left: 8px;
    border: 1px solid var(--border, #3e3e42);
    border-radius: 4px;
    overflow: hidden;
  }
  .view-btn {
    padding: 3px 10px;
    font-size: 11px;
    background: transparent;
    border: none;
    color: var(--fg-secondary, #a0a0a0);
    cursor: pointer;
  }
  .view-btn:hover { background: var(--bg-tertiary, #2d2d30); }
  .view-btn.active {
    background: var(--accent, #59a8dd);
    color: var(--text-on-accent, #fff);
  }

  /* ── List view (plans/reviews) ── */
  .list-view {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px;
  }
  .list-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
    color: var(--fg-primary, #e0e0e0);
  }
  .list-item:hover { background: var(--bg-tertiary, #2d2d30); }
  .list-item .list-name { flex: 1; }
  .list-empty {
    padding: 24px 12px;
    text-align: center;
    font-size: 12px;
    color: var(--fg-muted, #9aa1a9);
    font-style: italic;
  }

  /* ── Board ── */
  .board {
    display: flex;
    flex: 1;
    gap: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .column {
    flex: 1;
    min-width: 140px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border, #3e3e42);
  }
  .column:last-child { border-right: none; }
  .col-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--fg-secondary, #a0a0a0);
    border-top: 3px solid var(--border, #3e3e42);
    flex-shrink: 0;
  }
  .col-header.pending { border-top-color: var(--fg-muted, #9aa1a9); }
  .col-header.ready { border-top-color: var(--accent, #59a8dd); }
  .col-header.in_progress { border-top-color: var(--warning, #dcdcaa); }
  .col-header.blocked { border-top-color: var(--error, #f48771); }
  .col-header.complete { border-top-color: var(--success, #4ec9b0); }
  .col-header.wontfix { border-top-color: var(--fg-muted, #9aa1a9); }
  .col-count {
    background: var(--bg-tertiary, #2d2d30);
    padding: 0 6px;
    border-radius: 8px;
    font-size: 10px;
    min-width: 18px;
    text-align: center;
  }
  .col-body {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
  }

  /* ── Cards ── */
  .card {
    margin-bottom: 4px;
    cursor: grab;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .card:hover { transform: none; }
  .card.dragging { opacity: 0.4; }
  .drag-ghost {
    position: fixed;
    pointer-events: none;
    z-index: 1000;
    opacity: 0.85;
    transform: rotate(2deg);
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    max-width: 200px;
  }
  .card-header {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 2px;
  }
  .card-id {
    font-size: 10px;
    font-weight: 600;
    color: var(--fg-muted, #9aa1a9);
    font-family: "JetBrains Mono", monospace;
  }
  .card-title {
    font-size: 12px;
    line-height: 1.35;
  }
  .worklog-badge { font-size: 10px; color: var(--success, #4ec9b0); margin-left: auto; }
  .dep-badge { font-size: 10px; color: var(--fg-muted, #9aa1a9); }

  /* ── Drop target ── */
  .column.drop-target .col-body {
    background: color-mix(in srgb, var(--accent, #59a8dd) 8%, transparent);
  }

  /* ── Empty column ── */
  .empty-col {
    padding: 16px 8px;
    text-align: center;
    font-size: 11px;
    color: var(--fg-muted, #9aa1a9);
    font-style: italic;
  }

  /* ── Pending changes bar ── */
  .pending-bar {
    border-top: 2px solid var(--warning, #dcdcaa);
    background: var(--bg-secondary, #252526);
    padding: 8px 12px;
    flex-shrink: 0;
  }
  .pending-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  .pending-count {
    font-size: 12px;
    font-weight: 600;
    color: var(--warning, #dcdcaa);
  }
  .pending-actions { display: flex; gap: 6px; }
  .pending-list { max-height: 80px; overflow-y: auto; }
  .change-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 0;
    font-size: 11px;
    color: var(--fg-secondary, #a0a0a0);
  }
  .change-undo {
    padding: 0 4px;
    font-size: 10px;
    background: transparent;
    border: none;
    color: var(--fg-muted, #9aa1a9);
    cursor: pointer;
  }
  .change-undo:hover { color: var(--error, #f48771); }

  /* ── Toast ── */
  .toast {
    position: fixed;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    padding: 6px 14px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    opacity: 0;
    transition: opacity 0.2s;
    pointer-events: none;
    z-index: 100;
  }
  .toast.show { opacity: 1; }
  .toast.error { background: var(--error, #f48771); color: var(--text-on-error, #000); }
  .toast.success { background: var(--success, #4ec9b0); color: var(--text-on-success, #000); }

  /* ── Archive button ── */
  .archive-btn {
    margin-left: auto;
    padding: 3px 10px;
    font-size: 11px;
    background: transparent;
    border: 1px solid var(--border, #3e3e42);
    border-radius: 4px;
    color: var(--fg-secondary, #a0a0a0);
    cursor: pointer;
  }
  .archive-btn:hover {
    background: var(--bg-tertiary, #2d2d30);
    color: var(--fg-primary, #e0e0e0);
  }
  .archive-btn:disabled { opacity: 0.5; cursor: wait; }
</style>
</head>
<body>
  <div class="filter-bar">
    <input type="search" class="search-input" placeholder="Search stories..." value="${esc(search || "")}">
    ${renderViewSwitcher(view)}
    <button id="btn-archive-old" class="archive-btn" title="Move complete/wontfix stories older than 5 days to stories/archive/">Archive &gt;5d</button>
  </div>
  <div class="board">
    ${columns}
  </div>
  ${renderPendingChanges()}
  <div class="toast" id="toast"></div>

<script>
  // ── View switcher ──
  document.querySelectorAll(".view-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      window.parent.postMessage({ type: "view-change", view: btn.dataset.view }, "*");
    });
  });

  // ── Filter ──
  const searchInput = document.querySelector(".search-input");
  searchInput.addEventListener("input", () => {
    window.parent.postMessage({ type: "filter-change", search: searchInput.value }, "*");
  });

  // ── Drag and Drop ──
  // Uses mouse events on document with a movement threshold to
  // distinguish clicks from drags.
  const DRAG_THRESHOLD = 5;
  let drag = null;

  function getColumnAt(x, y) {
    for (const col of document.querySelectorAll(".column")) {
      const r = col.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return col;
    }
    return null;
  }

  function cleanupDrag() {
    if (!drag) return;
    if (drag.ghost) drag.ghost.remove();
    if (drag.card) drag.card.classList.remove("dragging");
    document.querySelectorAll(".column.drop-target").forEach((c) => c.classList.remove("drop-target"));
    drag = null;
  }

  document.querySelectorAll(".card").forEach((card) => {
    card.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      drag = {
        card,
        ghost: null,
        data: {
          storyId: card.dataset.storyId,
          filename: card.dataset.filename,
          status: card.dataset.status,
          title: card.dataset.title,
        },
        startX: e.clientX,
        startY: e.clientY,
        started: false,
      };
    });
  });

  document.addEventListener("mousemove", (e) => {
    if (!drag) return;
    const dx = e.clientX - drag.startX;
    const dy = e.clientY - drag.startY;

    if (!drag.started) {
      if (Math.abs(dx) + Math.abs(dy) < DRAG_THRESHOLD) return;
      drag.started = true;
      drag.card.classList.add("dragging");
      const ghost = drag.card.cloneNode(true);
      ghost.classList.add("drag-ghost");
      ghost.classList.remove("dragging");
      ghost.style.width = drag.card.getBoundingClientRect().width + "px";
      document.body.appendChild(ghost);
      drag.ghost = ghost;
    }

    drag.ghost.style.left = (e.clientX - 20) + "px";
    drag.ghost.style.top = (e.clientY - 10) + "px";

    document.querySelectorAll(".column.drop-target").forEach((c) => c.classList.remove("drop-target"));
    const col = getColumnAt(e.clientX, e.clientY);
    if (col && col.dataset.status !== drag.data.status) {
      col.classList.add("drop-target");
    }
  });

  document.addEventListener("mouseup", (e) => {
    if (!drag) return;

    if (!drag.started) {
      const filename = drag.data.filename;
      cleanupDrag();
      window.parent.postMessage({ type: "open-story", filename }, "*");
      return;
    }

    const col = getColumnAt(e.clientX, e.clientY);
    if (col) {
      const newStatus = col.dataset.status;
      if (newStatus !== drag.data.status) {
        window.parent.postMessage({
          type: "status-change",
          storyId: drag.data.storyId,
          filename: drag.data.filename,
          oldStatus: drag.data.status,
          newStatus,
          title: drag.data.title,
        }, "*");
      }
    }
    cleanupDrag();
  });

  // ── Pending changes actions ──
  const applyBtn = document.getElementById("btn-apply");
  if (applyBtn) {
    applyBtn.addEventListener("click", () => {
      window.parent.postMessage({ type: "apply-to-claude" }, "*");
    });
  }

  const discardBtn = document.getElementById("btn-discard");
  if (discardBtn) {
    discardBtn.addEventListener("click", () => {
      window.parent.postMessage({ type: "discard-changes" }, "*");
    });
  }

  document.querySelectorAll(".change-undo").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      window.parent.postMessage({ type: "undo-change", idx: parseInt(btn.dataset.idx, 10) }, "*");
    });
  });

  // ── Archive old ──
  const archiveBtn = document.getElementById("btn-archive-old");
  if (archiveBtn) {
    archiveBtn.addEventListener("click", () => {
      archiveBtn.disabled = true;
      window.parent.postMessage({ type: "archive-old" }, "*");
    });
  }

  // ── Messages from host ──
  window.addEventListener("message", (e) => {
    if (!e.data || !e.data.type) return;
    if (e.data.type === "toast") {
      const toast = document.getElementById("toast");
      toast.textContent = e.data.message;
      toast.className = "toast " + (e.data.level || "error") + " show";
      setTimeout(() => toast.classList.remove("show"), 3000);
    }
  });
</script>
</body>
</html>`;
}

function renderNoStoriesDir() {
  return `<!DOCTYPE html>
<html>
<head></head>
<body>
  <div class="empty-state">
    No stories directory found
    <div class="hint">Create a <code>stories/</code> directory in your repository to get started.</div>
  </div>
</body>
</html>`;
}

// ── Status change logic ─────────────────────────────────────────────────

function updateFrontmatter(content, newStatus) {
  const now = new Date().toISOString();
  let updated = content.replace(
    /^(status:\s*).+$/m,
    `$1${newStatus}`,
  );
  updated = updated.replace(
    /^(updated:\s*).+$/m,
    `$1"${now}"`,
  );
  return updated;
}

function buildNewFilename(oldFilename, newStatus) {
  return oldFilename.replace(
    /^(\d+-[a-f0-9]+-)[\w]+(-.+)$/,
    `$1${newStatus}$2`,
  );
}

async function handleStatusChange(data) {
  const { filename, newStatus, storyId, oldStatus, title } = data;

  // Work log gate
  if (newStatus === "complete" || newStatus === "wontfix") {
    const story = stories.find((s) => s.id === storyId);
    if (story && !story.hasWorkLog) {
      if (panelHandle) {
        panelHandle.send({
          type: "toast",
          level: "error",
          message: `Cannot move to ${STATUS_LABELS[newStatus]}: add a work log entry first.`,
        });
      }
      return;
    }
  }

  try {
    const filePath = `${storiesDir}/${filename}`;
    const content = await hostRef.readFile(filePath);
    const updatedContent = updateFrontmatter(content, newStatus);
    const newFilename = buildNewFilename(filename, newStatus);
    const newPath = `${storiesDir}/${newFilename}`;

    await hostRef.writeFile(filePath, updatedContent);
    if (newFilename !== filename) {
      await hostRef.renamePath(filePath, newPath);
    }

    // Track the change for "Apply to Claude"
    pendingChanges.push({ storyId, title: title || storyId, oldStatus, newStatus });

    hostRef.log("info", `Moved ${storyId} to ${newStatus}`, { filename, newFilename });
  } catch (err) {
    hostRef.log("error", `Failed to change status for ${storyId}`, String(err));
    if (panelHandle) {
      panelHandle.send({ type: "toast", level: "error", message: `Failed to update: ${err}` });
    }
  }
}

async function applyToClaude() {
  if (pendingChanges.length === 0) return;

  const sessionId = hostRef.getActiveTerminalSessionId();
  if (!sessionId) {
    if (panelHandle) {
      panelHandle.send({ type: "toast", level: "error", message: "No active terminal session" });
    }
    return;
  }

  const lines = pendingChanges.map((c) =>
    `- ${c.storyId} "${c.title}": ${STATUS_LABELS[c.oldStatus]} → ${STATUS_LABELS[c.newStatus]}`
  );

  const message = [
    "The following story status changes were made via the Kanban board:",
    ...lines,
    "",
    "Please acknowledge these changes and take appropriate action.",
  ].join("\n");

  try {
    await hostRef.sendAgentInput(sessionId, message);
    pendingChanges = [];
    if (panelHandle) {
      panelHandle.send({ type: "toast", level: "success", message: "Sent to terminal" });
    }
    await refreshBoard();
  } catch (err) {
    hostRef.log("error", "Failed to write to PTY", String(err));
    if (panelHandle) {
      panelHandle.send({ type: "toast", level: "error", message: `Failed to send: ${err}` });
    }
  }
}

// ── Panel management ────────────────────────────────────────────────────

let filters = { search: "" };

async function loadListItems(subdir) {
  if (!hostRef || !repoRoot) return [];
  try {
    const files = await hostRef.listDirectory(`${repoRoot}/${subdir}`, "*.md");
    return files.map((filename) => ({
      filename,
      displayName: filename.replace(/\.md$/, ""),
    })).sort((a, b) => a.displayName.localeCompare(b.displayName));
  } catch {
    return [];
  }
}

/**
 * Load plan files with their frontmatter status, collapsed into the
 * 3 kanban columns (planning / active / done).
 */
async function loadPlans() {
  if (!hostRef || !repoRoot) return [];
  const dir = `${repoRoot}/plans`;
  let files;
  try {
    files = await hostRef.listDirectory(dir, "*.md");
  } catch {
    return [];
  }

  const BATCH_SIZE = 20;
  const results = [];
  for (let i = 0; i < files.length; i += BATCH_SIZE) {
    const batch = files.slice(i, i + BATCH_SIZE);
    const batchResults = await Promise.all(
      batch.map(async (filename) => {
        try {
          const content = await hostRef.readFile(`${dir}/${filename}`);
          const fm = parseFrontmatter(content);
          const rawStatus = (fm && fm.status) || "";
          const column = PLAN_STATUS_TO_COLUMN[rawStatus] || PLAN_FALLBACK_COLUMN;
          return {
            filename,
            displayName: filename.replace(/\.md$/, ""),
            rawStatus: rawStatus || "draft",
            column,
          };
        } catch {
          return null;
        }
      }),
    );
    results.push(...batchResults);
  }
  return results.filter(Boolean).sort((a, b) => a.displayName.localeCompare(b.displayName));
}

/** Load reviews with their frontmatter status. */
async function loadReviews() {
  if (!hostRef || !repoRoot) return [];
  const dir = `${repoRoot}/reviews`;
  let files;
  try {
    files = await hostRef.listDirectory(dir, "*.md");
  } catch {
    return [];
  }

  const BATCH_SIZE = 20;
  const results = [];
  for (let i = 0; i < files.length; i += BATCH_SIZE) {
    const batch = files.slice(i, i + BATCH_SIZE);
    const batchResults = await Promise.all(
      batch.map(async (filename) => {
        try {
          const content = await hostRef.readFile(`${dir}/${filename}`);
          const fm = parseFrontmatter(content);
          const rawStatus = (fm && fm.status) || "";
          const column = REVIEW_COLUMNS.includes(rawStatus) ? rawStatus : REVIEW_FALLBACK_COLUMN;
          return {
            filename,
            displayName: filename.replace(/\.md$/, ""),
            rawStatus: rawStatus || "open",
            column,
          };
        } catch {
          return null;
        }
      }),
    );
    results.push(...batchResults);
  }
  return results.filter(Boolean).sort((a, b) => b.displayName.localeCompare(a.displayName));
}

function renderPlanCard(plan) {
  return `<div class="card plan-card"
    data-filename="${esc(plan.filename)}"
    data-status="${esc(plan.rawStatus)}">
    <div class="card-title">${esc(plan.displayName)}</div>
    <div class="card-subtitle">${esc(plan.rawStatus)}</div>
  </div>`;
}

/**
 * Shared kanban renderer for plans + reviews. Cards are click-to-open only
 * (no drag-and-drop — neither surface has a stable mutation story like
 * stories' filename-encoded status).
 */
function renderGroupedBoard(items, cfg) {
  const { search } = filters;
  const searchLower = (search || "").toLowerCase();
  const filtered = items.filter((it) =>
    !searchLower || it.displayName.toLowerCase().includes(searchLower)
  );

  const byColumn = {};
  for (const c of cfg.columns) byColumn[c] = [];
  for (const it of filtered) {
    if (byColumn[it.column]) byColumn[it.column].push(it);
  }

  const columns = cfg.columns.map((c) => {
    const cards = byColumn[c].map(renderPlanCard).join("");
    const count = byColumn[c].length;
    const placeholder = count === 0 ? `<div class="empty-col">No ${esc(cfg.emptyNoun)}</div>` : "";
    return `<div class="column" data-status="${c}">
      <div class="col-header ${cfg.classPrefix}-${c}">
        <span class="col-label">${esc(cfg.labels[c])}</span>
        <span class="col-count">${count}</span>
      </div>
      <div class="col-body">${cards}${placeholder}</div>
    </div>`;
  }).join("");

  return `<!DOCTYPE html>
<html>
<head>
<style>
  body { overflow: hidden; height: 100vh; display: flex; flex-direction: column; }
  .search-input { flex: 1; max-width: 260px; }
  .view-switcher {
    display: inline-flex; gap: 2px; margin-left: 8px;
    border: 1px solid var(--border, #3e3e42); border-radius: 4px; overflow: hidden;
  }
  .view-btn {
    padding: 3px 10px; font-size: 11px; background: transparent; border: none;
    color: var(--fg-secondary, #a0a0a0); cursor: pointer;
  }
  .view-btn:hover { background: var(--bg-tertiary, #2d2d30); }
  .view-btn.active { background: var(--accent, #59a8dd); color: var(--text-on-accent, #fff); }

  .board { display: flex; flex: 1; gap: 0; overflow-x: auto; overflow-y: hidden; }
  .column {
    flex: 1; min-width: 200px; display: flex; flex-direction: column;
    border-right: 1px solid var(--border, #3e3e42);
  }
  .column:last-child { border-right: none; }
  .col-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 6px 10px; font-size: 11px; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--fg-secondary, #a0a0a0);
    border-top: 3px solid var(--border, #3e3e42);
    flex-shrink: 0;
  }
  .col-header.plan-planning { border-top-color: var(--fg-muted, #9aa1a9); }
  .col-header.plan-active { border-top-color: var(--warning, #dcdcaa); }
  .col-header.plan-done { border-top-color: var(--success, #4ec9b0); }
  .col-header.review-open { border-top-color: var(--warning, #dcdcaa); }
  .col-header.review-triaged { border-top-color: var(--success, #4ec9b0); }
  .col-count {
    background: var(--bg-tertiary, #2d2d30); padding: 0 6px;
    border-radius: 8px; font-size: 10px; min-width: 18px; text-align: center;
  }
  .col-body { flex: 1; overflow-y: auto; padding: 6px; }

  .card { margin-bottom: 4px; cursor: pointer; user-select: none; -webkit-user-select: none; }
  .card-title { font-size: 12px; line-height: 1.35; }
  .card-subtitle {
    font-size: 10px; color: var(--fg-muted, #9aa1a9);
    margin-top: 2px; font-family: "JetBrains Mono", monospace;
  }
  .empty-col {
    padding: 16px 8px; text-align: center; font-size: 11px;
    color: var(--fg-muted, #9aa1a9); font-style: italic;
  }
</style>
</head>
<body>
  <div class="filter-bar">
    <input type="search" class="search-input" placeholder="${esc(cfg.searchPlaceholder)}" value="${esc(search || "")}">
    ${renderViewSwitcher(view)}
  </div>
  <div class="board">${columns}</div>

<script>
  document.querySelectorAll(".view-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      window.parent.postMessage({ type: "view-change", view: btn.dataset.view }, "*");
    });
  });
  const searchInput = document.querySelector(".search-input");
  searchInput.addEventListener("input", () => {
    window.parent.postMessage({ type: "filter-change", search: searchInput.value }, "*");
  });
  document.querySelectorAll(".card").forEach((card) => {
    card.addEventListener("click", () => {
      window.parent.postMessage({ type: "open-list-item", filename: card.dataset.filename }, "*");
    });
  });
</script>
</body>
</html>`;
}

async function renderCurrentView() {
  if (view === "stories") {
    stories = await loadStories();
    return stories.length === 0 && !(await dirExists())
      ? renderNoStoriesDir()
      : renderBoard(stories, filters);
  }
  if (view === "plans") {
    const plans = await loadPlans();
    return renderGroupedBoard(plans, {
      columns: PLAN_COLUMNS,
      labels: PLAN_COLUMN_LABELS,
      classPrefix: "plan",
      emptyNoun: "plans",
      searchPlaceholder: "Search plans...",
    });
  }
  if (view === "reviews") {
    const reviews = await loadReviews();
    return renderGroupedBoard(reviews, {
      columns: REVIEW_COLUMNS,
      labels: REVIEW_COLUMN_LABELS,
      classPrefix: "review",
      emptyNoun: "reviews",
      searchPlaceholder: "Search reviews...",
    });
  }
  listItems = await loadListItems(VIEW_DIRS[view]);
  return renderList(listItems, filters);
}

async function openKanban() {
  const repo = hostRef.getActiveRepo();
  if (!repo) return;

  repoRoot = repo.path;
  storiesDir = `${repo.path}/${STORIES_DIR}`;

  const html = await renderCurrentView();

  panelHandle = hostRef.openPanel({
    id: "kanban-board",
    title: "Stories Kanban",
    html,
    onMessage: handlePanelMessage,
  });

  await startWatching();
}

async function dirExists() {
  try {
    const listing = await hostRef.listDirectory(storiesDir);
    return listing !== null;
  } catch {
    return false;
  }
}

async function refreshBoard() {
  if (!panelHandle || !repoRoot) return;
  const html = await renderCurrentView();
  panelHandle.update(html);
}

function handlePanelMessage(data) {
  if (!data || typeof data.type !== "string") return;

  if (data.type === "status-change") {
    handleStatusChange(data);
  }

  if (data.type === "open-story") {
    const filePath = `${storiesDir}/${data.filename}`;
    hostRef.openMarkdownFile(filePath);
  }

  if (data.type === "open-list-item") {
    if (!repoRoot) return;
    const filePath = `${repoRoot}/${VIEW_DIRS[view]}/${data.filename}`;
    hostRef.openMarkdownFile(filePath);
  }

  if (data.type === "view-change") {
    if (!VIEWS.includes(data.view) || data.view === view) return;
    view = data.view;
    filters = { search: "" };
    startWatching();
    refreshBoard();
  }

  if (data.type === "filter-change") {
    filters = { search: data.search || "" };
    refreshBoard();
  }

  if (data.type === "apply-to-claude") {
    applyToClaude();
  }

  if (data.type === "discard-changes") {
    pendingChanges = [];
    refreshBoard();
  }

  if (data.type === "undo-change") {
    const idx = data.idx;
    if (idx >= 0 && idx < pendingChanges.length) {
      pendingChanges.splice(idx, 1);
      refreshBoard();
    }
  }

  if (data.type === "archive-old") {
    archiveOldStories();
  }
}

/**
 * Move complete/wontfix stories whose `updated` (or fallback `created`)
 * timestamp is older than ARCHIVE_THRESHOLD_DAYS into `stories/archive/`.
 * The archive subfolder is expected to exist.
 */
const ARCHIVE_THRESHOLD_DAYS = 5;

async function archiveOldStories() {
  if (!storiesDir || !hostRef) return;
  const archiveDir = `${storiesDir}/archive`;
  const nowMs = Date.now();
  const cutoffMs = ARCHIVE_THRESHOLD_DAYS * 86400 * 1000;

  const current = stories.length > 0 ? stories : await loadStories();
  const candidates = [];
  for (const s of current) {
    if (s.status !== "complete" && s.status !== "wontfix") continue;
    const tsStr = s.updated || s.created;
    if (!tsStr) continue;
    const ts = Date.parse(tsStr);
    if (!Number.isFinite(ts)) continue;
    if (nowMs - ts > cutoffMs) candidates.push(s);
  }

  if (candidates.length === 0) {
    if (panelHandle) {
      panelHandle.send({ type: "toast", level: "success", message: "No stories to archive" });
    }
    await refreshBoard();
    return;
  }

  let moved = 0;
  const errors = [];
  for (const s of candidates) {
    try {
      await hostRef.renamePath(`${storiesDir}/${s.filename}`, `${archiveDir}/${s.filename}`);
      moved++;
    } catch (err) {
      errors.push(`${s.filename}: ${err}`);
    }
  }

  hostRef.log("info", `Archived ${moved}/${candidates.length} stories (>${ARCHIVE_THRESHOLD_DAYS}d)`);
  if (panelHandle) {
    const lvl = errors.length > 0 ? "error" : "success";
    const msg = errors.length > 0
      ? `Archived ${moved}, ${errors.length} failed`
      : `Archived ${moved} stor${moved === 1 ? "y" : "ies"}`;
    panelHandle.send({ type: "toast", level: lvl, message: msg });
  }
  if (errors.length > 0) {
    hostRef.log("error", `Archive errors`, errors);
  }
  await refreshBoard();
}

// ── File watching ───────────────────────────────────────────────────────

let debounceTimer = null;

async function startWatching() {
  if (watchDisposable) {
    watchDisposable.dispose();
    watchDisposable = null;
  }

  if (!repoRoot) return;
  const watchDir = `${repoRoot}/${VIEW_DIRS[view]}`;

  try {
    watchDisposable = await hostRef.watchPath(
      watchDir,
      () => {
        clearTimeout(debounceTimer);
        debounceTimer = setTimeout(refreshBoard, 500);
      },
      { recursive: false, debounceMs: 300 },
    );
  } catch {
    // The target directory may not exist yet (e.g. no reviews/ in repo)
  }
}

function stopWatching() {
  if (watchDisposable) {
    watchDisposable.dispose();
    watchDisposable = null;
  }
  clearTimeout(debounceTimer);
}

// ── Plugin lifecycle ────────────────────────────────────────────────────

export default {
  id: PLUGIN_ID,

  onload(host) {
    hostRef = host;

    host.registerTerminalAction({
      id: "open-kanban",
      label: "Stories Kanban",
      action: () => openKanban(),
    });

    host.onStateChange((event) => {
      if (event.type === "branch-changed" || event.type === "repo-changed") {
        stopWatching();
        pendingChanges = [];
        stories = [];
        listItems = [];
        storiesDir = null;
        repoRoot = null;
        if (panelHandle) {
          openKanban();
        }
      }
    });

    host.log("info", "Stories Kanban loaded");
  },

  onunload() {
    stopWatching();
    panelHandle = null;
    hostRef = null;
    stories = [];
    listItems = [];
    storiesDir = null;
    repoRoot = null;
    view = "stories";
    pendingChanges = [];
    filters = { search: "" };
  },
};
