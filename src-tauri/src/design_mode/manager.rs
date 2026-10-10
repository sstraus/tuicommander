//! Per-repository inspect-mode lifecycle. The CDP adapter is deliberately behind
//! a small port so a browser disconnect and a closed PTY can be tested without
//! launching Chrome or writing to a real terminal.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use chromiumoxide::cdp::browser_protocol::{css, dom, overlay, page};
use chromiumoxide::cdp::js_protocol::{debugger, runtime};
use chromiumoxide::{Browser, Page};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::payload::GrabPayload;
use super::source::{self, ScriptMaps};

const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;
/// One deadline for closing every browser at exit, not one per repository.
const STOP_ALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub(crate) type Prefill = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;
pub(crate) type Notify = Arc<dyn Fn(&ModeStatus) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModeStatus {
    pub(crate) repo_path: String,
    pub(crate) session_id: String,
    pub(crate) status: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Rect {
    fn from_value(value: &Value) -> Option<Self> {
        let number = |key| value.get(key)?.as_f64().filter(|number| number.is_finite());
        let rect = Self {
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
        };
        // A page can report arbitrarily large CSS boxes. Keep screenshot
        // allocation bounded; the textual grab still succeeds without an image.
        (rect.width > 0.0
            && rect.height > 0.0
            && rect.width <= 4096.0
            && rect.height <= 4096.0
            && rect.width * rect.height <= 4_194_304.0)
            .then_some(rect)
    }
}

pub(crate) struct Pick {
    pub(crate) raw: Value,
    pub(crate) styles: Value,
    pub(crate) rect: Value,
    pub(crate) source: Option<source::SourceLoc>,
}

pub(crate) trait InspectBrowser: Send + Sync {
    fn arm(&self) -> BoxFuture<'_, Result<(), String>>;
    /// Leaves inspect mode and drops every pick queued while nobody listened,
    /// so a later start cannot deliver a click the user made for another agent.
    fn disarm(&self) -> BoxFuture<'_, Result<(), String>>;
    fn next_pick(&self) -> BoxFuture<'_, Option<Pick>>;
    fn capture(&self, rect: Rect) -> BoxFuture<'_, Result<Option<Vec<u8>>, String>>;
    fn close(&self) -> BoxFuture<'_, Result<(), String>>;
}

struct Mode {
    status: ModeStatus,
    browser: Option<Arc<dyn InspectBrowser>>,
    listener: Option<JoinHandle<()>>,
    generation: u64,
}

#[derive(Clone)]
pub(crate) struct DesignModeManager {
    modes: Arc<Mutex<HashMap<String, Mode>>>,
    grab_dir: PathBuf,
    prefill: Prefill,
    notify: Notify,
}

impl DesignModeManager {
    pub(crate) fn new(grab_dir: PathBuf, prefill: Prefill, notify: Notify) -> Self {
        Self {
            modes: Arc::new(Mutex::new(HashMap::new())),
            grab_dir,
            prefill,
            notify,
        }
    }

    pub(crate) async fn statuses(&self) -> Vec<ModeStatus> {
        let mut statuses: Vec<_> = self
            .modes
            .lock()
            .await
            .values()
            .filter(|mode| mode.status.status != STARTING)
            .map(|mode| mode.status.clone())
            .collect();
        statuses.sort_by(|left, right| left.repo_path.cmp(&right.repo_path));
        statuses
    }

    /// The factory is called only when the repo has no usable browser. A second
    /// start on an armed repo simply changes the PTY bound to the same listener.
    ///
    /// Launching Chrome and arming take seconds, so the repo is marked
    /// "starting" and the `modes` lock is released meanwhile: other repos'
    /// picks, status reads and `stop` must not wait behind one launch.
    pub(crate) async fn start<F, Fut>(
        &self,
        repo_path: String,
        session_id: String,
        make_browser: F,
    ) -> Result<ModeStatus, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<dyn InspectBrowser>, String>>,
    {
        let (generation, reused, existed) = {
            let mut modes = self.modes.lock().await;
            match modes.get_mut(&repo_path) {
                Some(mode) if mode.status.status == "armed" => {
                    mode.status.session_id = session_id;
                    let status = mode.status.clone();
                    drop(modes);
                    (self.notify)(&status);
                    return Ok(status);
                }
                Some(mode) if mode.status.status == STARTING => {
                    return Err("Design Mode is already starting for this repository".into());
                }
                Some(mode) => {
                    mode.generation += 1;
                    mode.status.status = STARTING;
                    mode.status.session_id = session_id.clone();
                    (mode.generation, mode.browser.take(), true)
                }
                None => {
                    modes.insert(
                        repo_path.clone(),
                        Mode {
                            status: ModeStatus {
                                repo_path: repo_path.clone(),
                                session_id: session_id.clone(),
                                status: STARTING,
                            },
                            browser: None,
                            listener: None,
                            generation: 1,
                        },
                    );
                    (1, None, false)
                }
            }
        };

        let armed = arm_browser(reused, make_browser).await;

        let mut modes = self.modes.lock().await;
        let Some(mode) = modes
            .get_mut(&repo_path)
            .filter(|mode| mode.generation == generation)
        else {
            drop(modes);
            if let Ok(browser) = armed {
                let _ = browser.close().await;
            }
            return Err("Design Mode was stopped while it was starting".into());
        };
        let browser = match armed {
            Ok(browser) => browser,
            Err(error) => {
                if existed {
                    mode.status.status = "stopped";
                } else {
                    modes.remove(&repo_path);
                }
                return Err(error);
            }
        };
        mode.status.status = "armed";
        let status = mode.status.clone();
        let manager = self.clone();
        let key = repo_path.clone();
        let listening_browser = browser.clone();
        mode.listener = Some(tokio::spawn(async move {
            manager.listen(key, generation, listening_browser).await;
        }));
        mode.browser = Some(browser);
        drop(modes);
        (self.notify)(&status);
        Ok(status)
    }

    async fn listen(&self, repo_path: String, generation: u64, browser: Arc<dyn InspectBrowser>) {
        loop {
            let Some(pick) = browser.next_pick().await else {
                self.disconnected(&repo_path, generation).await;
                break;
            };
            if let Err(error) = self
                .handle_pick(&repo_path, generation, &browser, pick)
                .await
            {
                tracing::warn!(repo_path, %error, "Design Mode pick was not delivered");
            }
            if !self.is_armed(&repo_path, generation).await {
                break;
            }
            if let Err(error) = browser.arm().await {
                tracing::warn!(repo_path, %error, "Design Mode could not re-arm inspect mode");
                self.disconnected(&repo_path, generation).await;
                break;
            }
        }
    }

    async fn is_armed(&self, repo_path: &str, generation: u64) -> bool {
        self.modes
            .lock()
            .await
            .get(repo_path)
            .is_some_and(|mode| mode.generation == generation && mode.status.status == "armed")
    }

    async fn handle_pick(
        &self,
        repo_path: &str,
        generation: u64,
        browser: &Arc<dyn InspectBrowser>,
        pick: Pick,
    ) -> Result<(), String> {
        if !self.is_armed(repo_path, generation).await {
            return Ok(());
        }
        let png = match Rect::from_value(&pick.rect) {
            Some(rect) => {
                tokio::time::timeout(std::time::Duration::from_millis(350), browser.capture(rect))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                    .and_then(|bytes| self.save_png(&bytes).ok().flatten())
            }
            None => None,
        };
        let prompt = GrabPayload::from_raw(pick.raw, pick.styles, pick.rect)
            .to_prompt_with_source(png.as_deref(), pick.source.as_ref());
        let modes = self.modes.lock().await;
        let Some(mode) = modes.get(repo_path) else {
            return Ok(());
        };
        if mode.generation != generation || mode.status.status != "armed" {
            return Ok(());
        }
        (self.prefill)(&mode.status.session_id, &prompt)
    }

    fn save_png(&self, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
        if bytes.is_empty() || bytes.len() > MAX_PNG_BYTES {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.grab_dir).map_err(|error| error.to_string())?;
        let path = self.grab_dir.join(format!("{}.png", uuid::Uuid::now_v7()));
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(Some(path))
    }

    /// A browser that lost its connection or cannot re-arm is closed
    /// explicitly. Dropping the last `Arc` instead would SIGKILL a Chrome that
    /// TUIC launched (chromey sets `kill_on_drop`) and skip the graceful close.
    /// Only a healthy browser whose terminal closed is kept (`session_closed`).
    async fn disconnected(&self, repo_path: &str, generation: u64) {
        let mut modes = self.modes.lock().await;
        let Some(mode) = modes.get_mut(repo_path) else {
            return;
        };
        if mode.generation != generation || mode.status.status != "armed" {
            return;
        }
        mode.status.status = "stopped";
        let browser = mode.browser.take();
        let status = mode.status.clone();
        drop(modes);
        (self.notify)(&status);
        if let Some(browser) = browser
            && let Err(error) = browser.close().await
        {
            tracing::warn!(repo_path, %error, "Design Mode browser could not close");
        }
    }

    /// Closing the terminal disarms the mode but deliberately leaves Chrome
    /// alive, so the user can keep browsing the page. Inspect mode is turned
    /// off, otherwise the next click on the page would still be swallowed.
    pub(crate) async fn session_closed(&self, session_id: &str) {
        let mut changed = Vec::new();
        let mut listeners = Vec::new();
        let mut browsers = Vec::new();
        {
            let mut modes = self.modes.lock().await;
            for mode in modes.values_mut() {
                if mode.status.session_id == session_id
                    && matches!(mode.status.status, "armed" | STARTING)
                {
                    mode.status.status = "stopped";
                    mode.generation += 1;
                    listeners.extend(mode.listener.take());
                    browsers.extend(mode.browser.clone());
                    changed.push(mode.status.clone());
                }
            }
        }
        for listener in listeners {
            listener.abort();
            let _ = listener.await;
        }
        for status in changed {
            (self.notify)(&status);
        }
        for browser in browsers {
            if let Err(error) = browser.disarm().await {
                tracing::warn!(%error, "Design Mode could not leave inspect mode");
            }
        }
    }

    pub(crate) async fn stop(&self, repo_path: &str) -> Result<Option<ModeStatus>, String> {
        let (browser, status, changed) = {
            let mut modes = self.modes.lock().await;
            let Some(mode) = modes.get_mut(repo_path) else {
                return Ok(None);
            };
            let changed = mode.status.status != "stopped";
            mode.status.status = "stopped";
            mode.generation += 1;
            if let Some(listener) = mode.listener.take() {
                listener.abort();
            }
            (mode.browser.take(), mode.status.clone(), changed)
        };
        if changed {
            (self.notify)(&status);
        }
        if let Some(browser) = browser {
            browser.close().await?;
        }
        Ok(Some(status))
    }

    /// Closes every browser at once under one deadline, so exit waits at most
    /// `STOP_ALL_TIMEOUT` however many repositories hang. A browser still open
    /// at the deadline is dropped, and chromey's `kill_on_drop` ends it.
    pub(crate) async fn stop_all(&self) {
        let repos: Vec<String> = self.modes.lock().await.keys().cloned().collect();
        let stops = repos.iter().map(|repo| async move {
            if let Err(error) = self.stop(repo).await {
                tracing::warn!(repo_path = repo, %error, "Design Mode browser could not close");
            }
        });
        if tokio::time::timeout(STOP_ALL_TIMEOUT, futures_util::future::join_all(stops))
            .await
            .is_err()
        {
            tracing::warn!("Design Mode browsers did not close in time");
        }
    }
}

/// Status of a repo whose browser is launching. Internal only: `statuses()`
/// hides it, because the public contract is `armed` or `stopped`.
const STARTING: &str = "starting";

/// Arms a reused browser, or makes a new one. A reused browser that cannot
/// arm is dead (the user closed Chrome), so it is closed and replaced in the
/// same click instead of failing it.
async fn arm_browser<F, Fut>(
    reused: Option<Arc<dyn InspectBrowser>>,
    make_browser: F,
) -> Result<Arc<dyn InspectBrowser>, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Arc<dyn InspectBrowser>, String>>,
{
    if let Some(browser) = reused {
        match async {
            browser.disarm().await?;
            browser.arm().await
        }
        .await
        {
            Ok(()) => return Ok(browser),
            Err(error) => {
                tracing::info!(%error, "Design Mode replaces a browser that cannot arm");
                let _ = browser.close().await;
            }
        }
    }
    let browser = make_browser().await?;
    if let Err(error) = browser.arm().await {
        let _ = browser.close().await;
        return Err(error);
    }
    Ok(browser)
}

/// The concrete headed Chrome adapter. A dedicated profile is reused on a
/// second start; the handler task is owned here and ends with the browser.
struct CdpInspectBrowser {
    browser: Mutex<Browser>,
    page: Page,
    picks: Mutex<chromiumoxide::listeners::EventStream<overlay::EventInspectNodeRequested>>,
    maps: Arc<Mutex<ScriptMaps>>,
    handler_task: JoinHandle<()>,
    map_task: JoinHandle<()>,
}

impl CdpInspectBrowser {
    async fn connect(
        repo_root: &Path,
        dev_server_url: Option<&str>,
    ) -> Result<Arc<dyn InspectBrowser>, String> {
        let (browser, mut handler) = super::browser::launch_or_attach(repo_root).await?;
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let pages = browser.pages().await.map_err(|error| error.to_string())?;
        let mut page = None;
        for candidate in pages {
            let url = candidate.url().await.ok().flatten().unwrap_or_default();
            if url.starts_with("http://") || url.starts_with("https://") {
                page = Some((candidate, url));
                break;
            }
        }
        let new_page = page.is_none();
        let (page, page_url) = match page {
            Some((page, url)) => (page, Some(url)),
            None => (
                browser
                    .new_page("about:blank")
                    .await
                    .map_err(|error| error.to_string())?,
                None,
            ),
        };
        let picks = page
            .event_listener::<overlay::EventInspectNodeRequested>()
            .await
            .map_err(|error| error.to_string())?;
        let scripts = page
            .event_listener::<debugger::EventScriptParsed>()
            .await
            .map_err(|error| error.to_string())?;
        let navigations = page
            .event_listener::<page::EventFrameNavigated>()
            .await
            .map_err(|error| error.to_string())?;
        enable_domains(&page).await?;
        if new_page && let Some(url) = dev_server_url {
            page.goto(url).await.map_err(|error| error.to_string())?;
        }
        let scripts = scripts.filter_map(|event| {
            std::future::ready(
                event
                    .source_map_url
                    .clone()
                    .filter(|map| !map.is_empty())
                    .map(|source_map_url| source::PageEvent::ScriptParsed {
                        url: event.url.clone(),
                        source_map_url,
                        has_source_url: event.has_source_url.unwrap_or(false),
                    }),
            )
        });
        let navigations = navigations.filter_map(|event| {
            std::future::ready(
                event
                    .frame
                    .parent_id
                    .is_none()
                    .then(|| source::PageEvent::Navigated(event.frame.url.clone())),
            )
        });
        let events = Box::pin(futures_util::stream::select(scripts, navigations));
        let maps = Arc::new(Mutex::new(ScriptMaps::default()));
        let map_task = tokio::spawn(source::load_maps(
            events,
            maps.clone(),
            dev_server_url.map(str::to_owned),
            page_url,
        ));
        Ok(Arc::new(Self {
            browser: Mutex::new(browser),
            page,
            picks: Mutex::new(picks),
            maps,
            handler_task,
            map_task,
        }))
    }

    async fn extract(&self, backend_node_id: dom::BackendNodeId) -> Result<Pick, String> {
        let node = self
            .page
            .execute(dom::ResolveNodeParams {
                backend_node_id: Some(backend_node_id),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let object_id = node
            .object
            .object_id
            .clone()
            .ok_or("DOM node has no runtime object")?;
        let call = self
            .page
            .execute(runtime::CallFunctionOnParams {
                function_declaration: include_str!("extract.js").to_owned(),
                object_id: Some(object_id),
                return_by_value: Some(true),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        if call.exception_details.is_some() {
            return Err("DOM extraction failed".into());
        }
        let raw = call
            .result
            .result
            .value
            .ok_or("DOM extraction returned no value")?;
        let box_model = self
            .page
            .execute(dom::GetBoxModelParams {
                backend_node_id: Some(backend_node_id),
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let points = box_model.model.border.inner();
        if points.len() != 8 {
            return Err("DOM box model has no border quad".into());
        }
        let xs = [points[0], points[2], points[4], points[6]];
        let ys = [points[1], points[3], points[5], points[7]];
        let x = xs.into_iter().fold(f64::INFINITY, f64::min);
        let y = ys.into_iter().fold(f64::INFINITY, f64::min);
        let width = xs.into_iter().fold(f64::NEG_INFINITY, f64::max) - x;
        let height = ys.into_iter().fold(f64::NEG_INFINITY, f64::max) - y;
        let rect = json!({ "x": x, "y": y, "width": width, "height": height });
        let mut maps = self.maps.lock().await;
        let source = source::resolve(&raw["source"], &mut maps);
        Ok(Pick {
            styles: raw["styles"].clone(),
            raw,
            rect,
            source,
        })
    }
}

impl InspectBrowser for CdpInspectBrowser {
    fn arm(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.page
                .execute(overlay::SetInspectModeParams {
                    mode: overlay::InspectMode::SearchForNode,
                    highlight_config: Some(overlay::HighlightConfig {
                        show_info: Some(true),
                        ..Default::default()
                    }),
                })
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn disarm(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.page
                .execute(overlay::SetInspectModeParams {
                    mode: overlay::InspectMode::None,
                    highlight_config: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            // The handler routes events before the command reply, so every pick
            // made before inspect mode went off is queued by now.
            let mut picks = self.picks.lock().await;
            while let Some(Some(_)) = futures_util::FutureExt::now_or_never(picks.next()) {}
            Ok(())
        })
    }

    fn next_pick(&self) -> BoxFuture<'_, Option<Pick>> {
        Box::pin(async {
            loop {
                let event = self.picks.lock().await.next().await?;
                match self.extract(event.backend_node_id).await {
                    Ok(pick) => return Some(pick),
                    Err(error) => {
                        tracing::warn!(%error, "Design Mode could not extract selected node")
                    }
                }
            }
        })
    }

    fn capture(&self, rect: Rect) -> BoxFuture<'_, Result<Option<Vec<u8>>, String>> {
        Box::pin(async move {
            let clip = page::Viewport {
                x: rect.x.max(0.0),
                y: rect.y.max(0.0),
                width: rect.width,
                height: rect.height,
                scale: 1.0,
            };
            let response = self
                .page
                .execute(page::CaptureScreenshotParams {
                    format: Some(page::CaptureScreenshotFormat::Png),
                    clip: Some(clip),
                    capture_beyond_viewport: Some(false),
                    ..Default::default()
                })
                .await
                .map_err(|error| error.to_string())?;
            let encoded: &str = response.data.as_ref();
            if encoded.len() > MAX_PNG_BYTES * 2 {
                return Ok(None);
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| error.to_string())?;
            Ok((bytes.len() <= MAX_PNG_BYTES).then_some(bytes))
        })
    }

    fn close(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.map_task.abort();
            let mut browser = self.browser.lock().await;
            let graceful = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                browser.close().await.map_err(|error| error.to_string())?;
                browser.wait().await.map_err(|error| error.to_string())?;
                Ok::<(), String>(())
            })
            .await;
            if !matches!(&graceful, Ok(Ok(()))) {
                tracing::warn!(?graceful, "Design Mode Chrome needed forced shutdown");
                if let Some(result) = browser.kill().await {
                    result.map_err(|error| error.to_string())?;
                }
            }
            self.handler_task.abort();
            Ok(())
        })
    }
}

/// Enables the CDP domains Design Mode reads. Debugger is on only for
/// `scriptParsed`, but while it is enabled Chrome stops at every `debugger;`
/// statement and nothing here resumes, so the page would hang: skip all pauses.
async fn enable_domains(page: &Page) -> Result<(), String> {
    let error = |error: chromiumoxide::error::CdpError| error.to_string();
    page.execute(dom::EnableParams::default())
        .await
        .map_err(error)?;
    page.execute(css::EnableParams::default())
        .await
        .map_err(error)?;
    page.execute(overlay::EnableParams::default())
        .await
        .map_err(error)?;
    page.execute(debugger::EnableParams::default())
        .await
        .map_err(error)?;
    page.execute(debugger::SetSkipAllPausesParams { skip: true })
        .await
        .map_err(error)?;
    Ok(())
}

pub(crate) async fn live_browser(
    repo_root: &Path,
    dev_server_url: Option<&str>,
) -> Result<Arc<dyn InspectBrowser>, String> {
    // Shared tungstenite TLS features enlarge this future; keep it off callers' stacks.
    Box::pin(CdpInspectBrowser::connect(repo_root, dev_server_url)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    struct FakeBrowser {
        picks: Mutex<mpsc::UnboundedReceiver<Pick>>,
        armed: AtomicUsize,
        disarmed: AtomicUsize,
        /// The next N `arm` calls fail, as on a dead or detached browser.
        arm_failures: AtomicUsize,
        closed: AtomicUsize,
        close_delay: std::time::Duration,
        image: Option<Vec<u8>>,
        capture_delay: std::time::Duration,
    }

    impl InspectBrowser for FakeBrowser {
        fn arm(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async {
                let failing = self
                    .arm_failures
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                        left.checked_sub(1)
                    })
                    .is_ok();
                if failing {
                    return Err("target detached".into());
                }
                self.armed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
        fn disarm(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async {
                self.disarmed.fetch_add(1, Ordering::SeqCst);
                let mut picks = self.picks.lock().await;
                while picks.try_recv().is_ok() {}
                Ok(())
            })
        }
        fn next_pick(&self) -> BoxFuture<'_, Option<Pick>> {
            Box::pin(async { self.picks.lock().await.recv().await })
        }
        fn capture(&self, _rect: Rect) -> BoxFuture<'_, Result<Option<Vec<u8>>, String>> {
            Box::pin(async {
                tokio::time::sleep(self.capture_delay).await;
                Ok(self.image.clone())
            })
        }
        fn close(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async {
                tokio::time::sleep(self.close_delay).await;
                self.closed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    fn fake_with_delay(
        image: Option<Vec<u8>>,
        capture_delay: std::time::Duration,
    ) -> (Arc<FakeBrowser>, mpsc::UnboundedSender<Pick>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Arc::new(FakeBrowser {
                picks: Mutex::new(rx),
                armed: AtomicUsize::new(0),
                disarmed: AtomicUsize::new(0),
                arm_failures: AtomicUsize::new(0),
                closed: AtomicUsize::new(0),
                close_delay: std::time::Duration::ZERO,
                image,
                capture_delay,
            }),
            tx,
        )
    }

    fn fake(image: Option<Vec<u8>>) -> (Arc<FakeBrowser>, mpsc::UnboundedSender<Pick>) {
        fake_with_delay(image, std::time::Duration::ZERO)
    }

    fn manager(
        dir: PathBuf,
    ) -> (
        DesignModeManager,
        mpsc::UnboundedReceiver<(String, String)>,
        mpsc::UnboundedReceiver<ModeStatus>,
    ) {
        let (prefill_tx, prefill_rx) = mpsc::unbounded_channel();
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        let manager = DesignModeManager::new(
            dir,
            Arc::new(move |session, text| {
                prefill_tx
                    .send((session.to_owned(), text.to_owned()))
                    .map_err(|error| error.to_string())
            }),
            Arc::new(move |status| {
                let _ = notify_tx.send(status.clone());
            }),
        );
        (manager, prefill_rx, notify_rx)
    }

    fn pick() -> Pick {
        pick_of("#save")
    }

    fn pick_of(selector: &str) -> Pick {
        Pick {
            raw: json!({"selector":selector, "tagName":"button", "textContent":"Save"}),
            styles: json!({"color":"red"}),
            rect: json!({"x":1.0,"y":2.0,"width":30.0,"height":20.0}),
            source: None,
        }
    }

    async fn recv<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> T {
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn start_pick_prefills_once_and_rearms() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(Some(b"png-bytes".to_vec()));
        let status = manager
            .start("/repo".into(), "agent-1".into(), || async {
                Ok(browser.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        assert_eq!(status.status, "armed");
        assert_eq!(recv(&mut events).await.status, "armed");
        tx.send(pick()).unwrap();
        let (session, prompt) = recv(&mut prefilled).await;
        assert_eq!(session, "agent-1");
        assert!(prompt.contains("selector: #save"));
        assert!(prompt.contains("[image: "));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while browser.armed.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            prefilled.try_recv().is_err(),
            "one pick must produce one prefill"
        );
        manager.stop_all().await;
    }

    #[tokio::test]
    async fn disconnect_stops_and_notifies() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(None);
        manager
            .start("/repo".into(), "agent".into(), || async {
                Ok(browser as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        assert_eq!(recv(&mut events).await.status, "armed");
        drop(tx);
        assert_eq!(recv(&mut events).await.status, "stopped");
    }

    #[tokio::test]
    async fn session_close_refuses_later_picks_and_keeps_chrome_open() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(None);
        manager
            .start("/repo".into(), "agent".into(), || async {
                Ok(browser.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        recv(&mut events).await;
        manager.session_closed("agent").await;
        assert_eq!(recv(&mut events).await.status, "stopped");
        tx.send(pick()).unwrap();
        tokio::task::yield_now().await;
        assert!(prefilled.try_recv().is_err());
        assert_eq!(browser.closed.load(Ordering::SeqCst), 0);
        manager.stop_all().await;
        assert_eq!(browser.closed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn second_start_rebinds_same_browser_and_stop_all_closes_each() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, _) = manager(dir.path().to_path_buf());
        let (browser_a, tx_a) = fake(None);
        let (browser_b, _tx_b) = fake(None);
        manager
            .start("/a".into(), "old".into(), || async {
                Ok(browser_a.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        manager
            .start("/a".into(), "new".into(), || async {
                panic!("active repo must not create another browser");
                #[allow(unreachable_code)]
                Ok(browser_a.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        manager
            .start("/b".into(), "other".into(), || async {
                Ok(browser_b.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        tx_a.send(pick()).unwrap();
        assert_eq!(recv(&mut prefilled).await.0, "new");
        manager.stop_all().await;
        assert_eq!(browser_a.closed.load(Ordering::SeqCst), 1);
        assert_eq!(browser_b.closed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stalled_capture_omits_image_but_still_prefills_and_rearms() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, _) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake_with_delay(
            Some(b"png-bytes".to_vec()),
            std::time::Duration::from_secs(5),
        );
        manager
            .start("/repo".into(), "agent".into(), || async {
                Ok(browser.clone() as Arc<dyn InspectBrowser>)
            })
            .await
            .unwrap();
        tx.send(pick()).unwrap();
        let (_, prompt) = recv(&mut prefilled).await;
        assert!(prompt.contains("selector: #save"));
        assert!(!prompt.contains("[image:"));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while browser.armed.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        manager.stop_all().await;
    }

    async fn wait_until(condition: impl Fn() -> bool) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !condition() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    fn start_with(
        manager: &DesignModeManager,
        repo: &str,
        session: &str,
        browser: &Arc<FakeBrowser>,
    ) -> impl Future<Output = Result<ModeStatus, String>> {
        let browser = browser.clone();
        let (manager, repo, session) = (manager.clone(), repo.to_owned(), session.to_owned());
        async move {
            manager
                .start(repo, session, || async move {
                    Ok(browser as Arc<dyn InspectBrowser>)
                })
                .await
        }
    }

    #[tokio::test]
    async fn session_close_turns_inspect_off_and_restart_ignores_stale_picks() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, _) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(None);
        start_with(&manager, "/repo", "agent-1", &browser)
            .await
            .unwrap();
        manager.session_closed("agent-1").await;
        assert_eq!(
            browser.disarmed.load(Ordering::SeqCst),
            1,
            "the page must leave inspect mode when its terminal closes"
        );
        // A click queued while no listener ran belongs to nobody.
        tx.send(pick_of("#stale")).unwrap();
        manager
            .start("/repo".into(), "agent-2".into(), || async {
                panic!("a live browser is reused");
                #[allow(unreachable_code)]
                Err(String::new())
            })
            .await
            .unwrap();
        tx.send(pick_of("#fresh")).unwrap();
        let (session, prompt) = recv(&mut prefilled).await;
        assert_eq!(session, "agent-2");
        assert!(
            prompt.contains("#fresh"),
            "stale pick was delivered: {prompt}"
        );
        wait_until(|| browser.armed.load(Ordering::SeqCst) >= 3).await;
        assert!(prefilled.try_recv().is_err());
        manager.stop_all().await;
    }

    #[tokio::test]
    async fn a_slow_browser_launch_does_not_block_other_repos() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, _) = manager(dir.path().to_path_buf());
        let (slow, _slow_tx) = fake(None);
        let (fast, _fast_tx) = fake(None);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let starting = {
            let manager = manager.clone();
            let slow = slow.clone();
            tokio::spawn(async move {
                manager
                    .start("/slow".into(), "agent-a".into(), || async move {
                        let _ = entered_tx.send(());
                        let _ = release_rx.await;
                        Ok(slow as Arc<dyn InspectBrowser>)
                    })
                    .await
            })
        };
        entered_rx.await.unwrap();
        let bound = std::time::Duration::from_secs(1);
        let statuses = tokio::time::timeout(bound, manager.statuses())
            .await
            .expect("status read waited for another repo's Chrome launch");
        assert!(
            statuses.is_empty(),
            "a starting repo is not reported: {statuses:?}"
        );
        tokio::time::timeout(bound, start_with(&manager, "/fast", "agent-b", &fast))
            .await
            .expect("start on another repo waited for the launch")
            .unwrap();
        assert!(
            start_with(&manager, "/slow", "agent-c", &fast)
                .await
                .is_err(),
            "a second start while launching must not launch another browser"
        );
        release_tx.send(()).unwrap();
        assert_eq!(starting.await.unwrap().unwrap().status, "armed");
        manager.stop_all().await;
    }

    #[tokio::test]
    async fn stop_during_launch_closes_the_new_browser() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, _) = manager(dir.path().to_path_buf());
        let (browser, _tx) = fake(None);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let starting = {
            let manager = manager.clone();
            let browser = browser.clone();
            tokio::spawn(async move {
                manager
                    .start("/repo".into(), "agent".into(), || async move {
                        let _ = entered_tx.send(());
                        let _ = release_rx.await;
                        Ok(browser as Arc<dyn InspectBrowser>)
                    })
                    .await
            })
        };
        entered_rx.await.unwrap();
        assert_eq!(
            manager.stop("/repo").await.unwrap().unwrap().status,
            "stopped"
        );
        release_tx.send(()).unwrap();
        assert!(starting.await.unwrap().is_err());
        assert_eq!(browser.closed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rearm_failure_closes_the_browser_instead_of_dropping_it() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, mut events) = manager(dir.path().to_path_buf());
        let (browser, tx) = fake(None);
        start_with(&manager, "/repo", "agent", &browser)
            .await
            .unwrap();
        assert_eq!(recv(&mut events).await.status, "armed");
        browser.arm_failures.store(1, Ordering::SeqCst);
        tx.send(pick()).unwrap();
        recv(&mut prefilled).await;
        assert_eq!(recv(&mut events).await.status, "stopped");
        wait_until(|| browser.closed.load(Ordering::SeqCst) == 1).await;
    }

    #[tokio::test]
    async fn a_dead_reused_browser_is_replaced_in_the_same_start() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, mut prefilled, _) = manager(dir.path().to_path_buf());
        let (dead, _dead_tx) = fake(None);
        let (fresh, fresh_tx) = fake(None);
        start_with(&manager, "/repo", "agent", &dead).await.unwrap();
        manager.session_closed("agent").await;
        // The user closed Chrome while no terminal was bound.
        dead.arm_failures.store(usize::MAX, Ordering::SeqCst);
        let status = start_with(&manager, "/repo", "agent-2", &fresh)
            .await
            .unwrap();
        assert_eq!(status.status, "armed");
        assert_eq!(dead.closed.load(Ordering::SeqCst), 1);
        fresh_tx.send(pick()).unwrap();
        assert_eq!(recv(&mut prefilled).await.0, "agent-2");
        manager.stop_all().await;
    }

    #[tokio::test(start_paused = true)]
    async fn stop_all_closes_every_browser_at_once_under_one_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, _) = manager(dir.path().to_path_buf());
        let mut senders = Vec::new();
        for repo in ["/a", "/b", "/c"] {
            let (tx, rx) = mpsc::unbounded_channel();
            senders.push(tx);
            let hung = Arc::new(FakeBrowser {
                picks: Mutex::new(rx),
                armed: AtomicUsize::new(0),
                disarmed: AtomicUsize::new(0),
                arm_failures: AtomicUsize::new(0),
                closed: AtomicUsize::new(0),
                close_delay: std::time::Duration::from_secs(60),
                image: None,
                capture_delay: std::time::Duration::ZERO,
            });
            start_with(&manager, repo, "agent", &hung).await.unwrap();
        }
        let began = tokio::time::Instant::now();
        manager.stop_all().await;
        assert!(
            began.elapsed() <= STOP_ALL_TIMEOUT + std::time::Duration::from_millis(100),
            "exit waited {:?} for hung browsers",
            began.elapsed()
        );
        assert!(
            manager
                .statuses()
                .await
                .iter()
                .all(|status| status.status == "stopped")
        );
    }

    #[tokio::test]
    #[ignore = "needs an installed Chrome (TUIC_DESIGN_CHROME)"]
    async fn a_debugger_statement_does_not_freeze_the_inspected_page() {
        let chrome = std::env::var_os("TUIC_DESIGN_CHROME")
            .expect("TUIC_DESIGN_CHROME must point to an installed Chrome executable");
        let profile = tempfile::tempdir().expect("temporary Chrome profile");
        let config = chromiumoxide::BrowserConfig::builder()
            .chrome_executable(chrome)
            .user_data_dir(profile.path())
            .port(0)
            .build()
            .expect("Chrome config");
        let (mut browser, mut handler) = Browser::launch(config).await.expect("Chrome launches");
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let page = browser.new_page("about:blank").await.expect("test page");
        enable_domains(&page)
            .await
            .expect("Design Mode domains enabled");
        page.evaluate("setTimeout(() => { debugger; window.after = true; }, 0)")
            .await
            .expect("schedule a debugger statement");
        let resumed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let after = page.evaluate("Boolean(window.after)").await;
                if after.is_ok_and(|result| result.value() == Some(&json!(true))) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        let _ = browser.close().await;
        let _ = browser.kill().await;
        handler_task.abort();
        assert!(
            resumed.is_ok(),
            "the page stopped at `debugger;` while Design Mode was attached"
        );
    }

    #[test]
    fn invalid_rect_and_large_png_fail_closed() {
        assert!(Rect::from_value(&json!({"x":0,"y":0,"width":0,"height":10})).is_none());
        assert!(Rect::from_value(&json!({"x":0,"y":0,"width":1,"height":"NaN"})).is_none());
        assert!(Rect::from_value(&json!({"x":0,"y":0,"width":10000,"height":10000})).is_none());
        let dir = tempfile::tempdir().unwrap();
        let (manager, _, _) = manager(dir.path().to_path_buf());
        assert!(
            manager
                .save_png(&vec![0; MAX_PNG_BYTES + 1])
                .unwrap()
                .is_none()
        );
        assert!(!dir.path().read_dir().unwrap().any(|entry| entry.is_ok()));
    }
}
