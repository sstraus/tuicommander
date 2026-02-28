/**
 * Stories Kanban Plugin — Kanban board for file-stories
 *
 * Renders stories as draggable cards across 6 status columns.
 * Drag-and-drop triggers file rename + frontmatter update via
 * the plugin panel message bridge (onMessage/send).
 *
 * Capabilities: fs:read, fs:list, fs:watch, fs:write, fs:rename,
 *               ui:panel, ui:markdown
 */

const PLUGIN_ID = "wiz-stories-kanban";
const SECTION_ID = "kanban";
const STORIES_DIR = "stories";

const STATUSES = ["pending", "ready", "in_progress", "blocked", "complete", "wontfix"];

const STATUS_LABELS = {
  pending: "Pending",
  ready: "Ready",
  in_progress: "In Progress",
  blocked: "Blocked",
  complete: "Complete",
  wontfix: "Won\u2019t Fix",
};

const ICON_KANBAN = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M1.75 0h12.5C15.216 0 16 .784 16 1.75v12.5A1.75 1.75 0 0 1 14.25 16H1.75A1.75 1.75 0 0 1 0 14.25V1.75C0 .784.784 0 1.75 0M1.5 1.75v12.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V1.75a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25M5.25 2a.75.75 0 0 1 .75.75v7.5a.75.75 0 0 1-1.5 0v-7.5A.75.75 0 0 1 5.25 2m5.5 0a.75.75 0 0 1 .75.75v4.5a.75.75 0 0 1-1.5 0v-4.5a.75.75 0 0 1 .75-.75"/></svg>`;

// ── Plugin state ────────────────────────────────────────────────────────

let hostRef = null;
let panelHandle = null;
let watchDisposable = null;
let storiesDir = null;
let stories = [];

// ── Filename parsing ────────────────────────────────────────────────────

/**
 * Parse a story filename into its components.
 * Format: {seq}-{hash}-{status}-{priority}-{title-slug}.md
 * Example: 391-ff66-in_progress-P3-show-prs-for-remote-only-branches.md
 */
function parseFilename(filename) {
  // seq-hash-status-priority-rest.md
  const m = filename.match(/^(\d+)-([a-f0-9]+)-([\w]+)-(P[1-3])-(.+)\.md$/);
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
 * Uses simple regex — the YAML structure is well-known.
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

/**
 * Check if a story has at least one work log entry (### heading under ## Work Log).
 */
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

  // Load files in batches of 20
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

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderCard(story) {
  const priClass = story.priority.toLowerCase();
  const worklogBadge = story.hasWorkLog
    ? `<span class="worklog-badge" title="Has work log">&#10003;</span>`
    : "";

  return `<div class="card" draggable="true"
    data-story-id="${escapeHtml(story.id)}"
    data-filename="${escapeHtml(story.filename)}"
    data-status="${escapeHtml(story.status)}">
    <div class="card-header">
      <span class="card-id">${escapeHtml(story.id.split("-")[0])}</span>
      <span class="card-pri ${priClass}">${escapeHtml(story.priority)}</span>
      ${worklogBadge}
    </div>
    <div class="card-title">${escapeHtml(story.title)}</div>
  </div>`;
}

function renderBoard(storyList, filters) {
  const { search, priorities } = filters;
  const searchLower = (search || "").toLowerCase();

  const filtered = storyList.filter((s) => {
    if (searchLower && !s.title.toLowerCase().includes(searchLower) && !s.id.toLowerCase().includes(searchLower)) {
      return false;
    }
    if (priorities.length > 0 && !priorities.includes(s.priority)) {
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

  // Priority filter buttons
  const priButtons = ["P1", "P2", "P3"]
    .map((p) => {
      const active = priorities.includes(p) ? " active" : "";
      return `<button class="pri-btn ${p.toLowerCase()}${active}" data-priority="${p}">${p}</button>`;
    })
    .join("");

  return `<!DOCTYPE html>
<html>
<head>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    font-size: 13px;
    color: var(--fg-primary, #cdd6f4);
    background: var(--bg-primary, #1e1e2e);
    overflow: hidden;
    height: 100vh;
    display: flex;
    flex-direction: column;
  }

  /* ── Filter bar ── */
  .filter-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-primary, #45475a);
    flex-shrink: 0;
  }
  .search-input {
    flex: 1;
    max-width: 260px;
    padding: 4px 8px;
    background: var(--bg-tertiary, #313244);
    border: 1px solid var(--border-primary, #45475a);
    border-radius: 4px;
    color: var(--fg-primary, #cdd6f4);
    font-size: 12px;
    outline: none;
  }
  .search-input:focus { border-color: var(--accent-primary, #89b4fa); }
  .search-input::placeholder { color: var(--fg-muted, #6c7086); }
  .pri-btn {
    padding: 2px 8px;
    border: 1px solid var(--border-primary, #45475a);
    border-radius: 3px;
    background: transparent;
    color: var(--fg-secondary, #a6adc8);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }
  .pri-btn:hover { background: var(--bg-tertiary, #313244); }
  .pri-btn.active.p1 { background: var(--error, #f38ba8); color: var(--bg-primary, #1e1e2e); border-color: var(--error, #f38ba8); }
  .pri-btn.active.p2 { background: var(--warning, #f9e2af); color: var(--bg-primary, #1e1e2e); border-color: var(--warning, #f9e2af); }
  .pri-btn.active.p3 { background: var(--fg-muted, #6c7086); color: var(--bg-primary, #1e1e2e); border-color: var(--fg-muted, #6c7086); }

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
    min-width: 160px;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border-primary, #45475a);
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
    color: var(--fg-secondary, #a6adc8);
    border-top: 3px solid var(--border-primary, #45475a);
    flex-shrink: 0;
  }
  .col-header.pending { border-top-color: var(--fg-muted, #6c7086); }
  .col-header.ready { border-top-color: var(--accent-primary, #89b4fa); }
  .col-header.in_progress { border-top-color: var(--warning, #f9e2af); }
  .col-header.blocked { border-top-color: var(--error, #f38ba8); }
  .col-header.complete { border-top-color: var(--success, #a6e3a1); }
  .col-header.wontfix { border-top-color: var(--fg-muted, #6c7086); }
  .col-count {
    background: var(--bg-tertiary, #313244);
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
    background: var(--bg-secondary, #181825);
    border: 1px solid var(--border-primary, #45475a);
    border-radius: 4px;
    padding: 8px;
    margin-bottom: 4px;
    cursor: grab;
    transition: transform 0.1s, box-shadow 0.1s;
  }
  .card:hover {
    transform: translateY(-1px);
    box-shadow: 0 2px 6px rgba(0,0,0,0.3);
  }
  .card.dragging { opacity: 0.6; }
  .card-header {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 4px;
  }
  .card-id {
    font-size: 10px;
    font-weight: 600;
    color: var(--fg-muted, #6c7086);
    font-family: monospace;
  }
  .card-pri {
    font-size: 9px;
    font-weight: 700;
    padding: 0 4px;
    border-radius: 2px;
    line-height: 1.5;
  }
  .card-pri.p1 { background: var(--error, #f38ba8); color: var(--bg-primary, #1e1e2e); }
  .card-pri.p2 { background: var(--warning, #f9e2af); color: var(--bg-primary, #1e1e2e); }
  .card-pri.p3 { background: var(--fg-muted, #6c7086); color: var(--bg-primary, #1e1e2e); }
  .worklog-badge {
    font-size: 10px;
    color: var(--success, #a6e3a1);
    margin-left: auto;
  }
  .card-title {
    font-size: 12px;
    line-height: 1.35;
    color: var(--fg-primary, #cdd6f4);
  }

  /* ── Drop target ── */
  .column.drop-target .col-body {
    background: var(--bg-highlight, rgba(137,180,250,0.08));
  }

  /* ── Empty states ── */
  .empty-col {
    padding: 16px 8px;
    text-align: center;
    font-size: 11px;
    color: var(--fg-muted, #6c7086);
    font-style: italic;
  }
  .no-stories {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--fg-muted, #6c7086);
    font-size: 14px;
  }

  /* ── Error toast ── */
  .error-toast {
    position: fixed;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--error, #f38ba8);
    color: var(--bg-primary, #1e1e2e);
    padding: 6px 14px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    opacity: 0;
    transition: opacity 0.2s;
    pointer-events: none;
  }
  .error-toast.show { opacity: 1; }
</style>
</head>
<body>
  <div class="filter-bar">
    <input type="text" class="search-input" placeholder="Search stories..." value="${escapeHtml(search || "")}">
    ${priButtons}
  </div>
  <div class="board">
    ${columns}
  </div>
  <div class="error-toast" id="error-toast"></div>

<script>
  // ── State ──
  let currentSearch = "${escapeHtml(search || "")}";
  let currentPriorities = ${JSON.stringify(priorities)};

  // ── Filter handlers ──
  const searchInput = document.querySelector(".search-input");
  searchInput.addEventListener("input", () => {
    currentSearch = searchInput.value;
    window.parent.postMessage({ type: "filter-change", search: currentSearch, priorities: currentPriorities }, "*");
  });

  document.querySelectorAll(".pri-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      const p = btn.dataset.priority;
      const idx = currentPriorities.indexOf(p);
      if (idx >= 0) {
        currentPriorities.splice(idx, 1);
        btn.classList.remove("active");
      } else {
        currentPriorities.push(p);
        btn.classList.add("active");
      }
      window.parent.postMessage({ type: "filter-change", search: currentSearch, priorities: [...currentPriorities] }, "*");
    });
  });

  // ── Card click → open story ──
  document.querySelectorAll(".card").forEach((card) => {
    card.addEventListener("click", (e) => {
      if (card.classList.contains("dragging")) return;
      const filename = card.dataset.filename;
      window.parent.postMessage({ type: "open-story", filename }, "*");
    });
  });

  // ── Drag and Drop ──
  let dragData = null;

  document.querySelectorAll(".card").forEach((card) => {
    card.addEventListener("dragstart", (e) => {
      dragData = {
        storyId: card.dataset.storyId,
        filename: card.dataset.filename,
        status: card.dataset.status,
      };
      card.classList.add("dragging");
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", card.dataset.storyId);
    });

    card.addEventListener("dragend", () => {
      card.classList.remove("dragging");
      document.querySelectorAll(".column.drop-target").forEach((col) => col.classList.remove("drop-target"));
      dragData = null;
    });
  });

  document.querySelectorAll(".column").forEach((col) => {
    col.addEventListener("dragover", (e) => {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      col.classList.add("drop-target");
    });

    col.addEventListener("dragleave", (e) => {
      if (!col.contains(e.relatedTarget)) {
        col.classList.remove("drop-target");
      }
    });

    col.addEventListener("drop", (e) => {
      e.preventDefault();
      col.classList.remove("drop-target");
      if (!dragData) return;

      const newStatus = col.dataset.status;
      if (newStatus === dragData.status) return;

      window.parent.postMessage({
        type: "status-change",
        storyId: dragData.storyId,
        filename: dragData.filename,
        oldStatus: dragData.status,
        newStatus,
      }, "*");
    });
  });

  // ── Messages from host ──
  window.addEventListener("message", (e) => {
    if (!e.data || !e.data.type) return;
    if (e.data.type === "error") {
      showError(e.data.message);
    }
  });

  function showError(msg) {
    const toast = document.getElementById("error-toast");
    toast.textContent = msg;
    toast.classList.add("show");
    setTimeout(() => toast.classList.remove("show"), 3000);
  }
</script>
</body>
</html>`;
}

function renderNoStoriesDir() {
  return `<!DOCTYPE html>
<html>
<head>
<style>
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    color: var(--fg-muted, #6c7086);
    background: var(--bg-primary, #1e1e2e);
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    margin: 0;
  }
  .message { text-align: center; font-size: 14px; }
  .hint { font-size: 12px; margin-top: 8px; opacity: 0.7; }
</style>
</head>
<body>
  <div class="message">
    No stories directory found
    <div class="hint">Create a <code>stories/</code> directory in your repository to get started.</div>
  </div>
</body>
</html>`;
}

// ── Status change logic ─────────────────────────────────────────────────

/**
 * Update story frontmatter: set new status and updated timestamp.
 */
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

/**
 * Build new filename from old filename and new status.
 */
function buildNewFilename(oldFilename, newStatus) {
  // seq-hash-oldStatus-priority-slug.md
  return oldFilename.replace(
    /^(\d+-[a-f0-9]+-)[\w]+(-.+)$/,
    `$1${newStatus}$2`,
  );
}

async function handleStatusChange(data) {
  const { filename, newStatus, storyId } = data;

  // Work log gate: block complete/wontfix without worklog
  if (newStatus === "complete" || newStatus === "wontfix") {
    const story = stories.find((s) => s.id === storyId);
    if (story && !story.hasWorkLog) {
      if (panelHandle) {
        panelHandle.send({
          type: "error",
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

    // Write updated content to old path, then rename
    await hostRef.writeFile(filePath, updatedContent);
    await hostRef.renamePath(filePath, newPath);

    hostRef.log("info", `Moved ${storyId} to ${newStatus}`, { filename, newFilename });
  } catch (err) {
    hostRef.log("error", `Failed to change status for ${storyId}`, String(err));
    if (panelHandle) {
      panelHandle.send({ type: "error", message: `Failed to update: ${err}` });
    }
  }
}

// ── Panel management ────────────────────────────────────────────────────

let filters = { search: "", priorities: [] };

async function openKanban() {
  const repo = hostRef.getActiveRepo();
  if (!repo) return;

  storiesDir = `${repo.path}/${STORIES_DIR}`;
  stories = await loadStories();

  const html = stories.length === 0 && !(await dirExists())
    ? renderNoStoriesDir()
    : renderBoard(stories, filters);

  panelHandle = hostRef.openPanel({
    id: "kanban-board",
    title: "Stories Kanban",
    html,
    onMessage: handlePanelMessage,
  });

  // Start watching for file changes
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
  if (!panelHandle || !storiesDir) return;
  stories = await loadStories();

  const html = stories.length === 0 && !(await dirExists())
    ? renderNoStoriesDir()
    : renderBoard(stories, filters);

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

  if (data.type === "filter-change") {
    filters = {
      search: data.search || "",
      priorities: data.priorities || [],
    };
    refreshBoard();
  }
}

// ── File watching ───────────────────────────────────────────────────────

let debounceTimer = null;

async function startWatching() {
  if (watchDisposable) {
    watchDisposable.dispose();
    watchDisposable = null;
  }

  if (!storiesDir) return;

  try {
    watchDisposable = await hostRef.watchPath(
      storiesDir,
      () => {
        clearTimeout(debounceTimer);
        debounceTimer = setTimeout(refreshBoard, 500);
      },
      { recursive: false, debounceMs: 300 },
    );
  } catch {
    // stories/ may not exist yet — that's fine
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

    host.registerSection({
      id: SECTION_ID,
      label: "STORIES",
      priority: 40,
      canDismissAll: false,
    });

    host.addItem({
      id: `${PLUGIN_ID}:open`,
      pluginId: PLUGIN_ID,
      sectionId: SECTION_ID,
      title: "Stories Kanban",
      subtitle: "Open Kanban board",
      icon: ICON_KANBAN,
      dismissible: false,
      onClick: openKanban,
    });

    // Re-initialize when repo changes
    host.onStateChange((event) => {
      if (event.type === "branch-changed") {
        stopWatching();
        if (panelHandle) {
          // Reload with new repo context
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
    storiesDir = null;
    filters = { search: "", priorities: [] };
  },
};
