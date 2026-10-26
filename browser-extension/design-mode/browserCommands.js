// Chrome owns host permissions and reviewed-tab identity. No debugger/CDP access.
export async function browserCommand(command, chrome, tabId, errors) {
  if (!tabId) throw new Error('no_reviewed_tab: Open the Design Mode sidebar on a page first.');
  const tab = await chrome.tabs.get(tabId);
  const allowed = async (raw, explicit = false) => {
    const url = new URL(raw);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('host_denied: Use an HTTP(S) URL without credentials.');
    const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
    if ((!local || explicit) && !(await chrome.permissions.contains({origins:[`${url.origin}/*`]}))) throw new Error('host_denied: Grant this host in the Design Mode extension first.');
    return url;
  };
  const {action, args = {}} = command;
  if (action === 'navigate') {
    await allowed(args.url);
    await chrome.tabs.update(tabId, {url:args.url});
    return {url:args.url};
  }
  await allowed(tab.url);
  if (action === 'reload') { await chrome.tabs.reload(tabId); return {reloaded:true}; }
  if (action === 'errors') {
    const local = ['localhost', '127.0.0.1', '[::1]'].includes(new URL(tab.url).hostname);
    if (!local && !hasExplicitHostGrant((await chrome.permissions.getAll()).origins, tab.url)) throw new Error('host_denied: Grant this host explicitly before reading page errors.');
    return {errors:await errors(tabId)};
  }
  if (action === 'screenshot') {
    if (!tab.active) throw new Error('tab_not_visible: The reviewed tab must be visible.');
    const image = await captureVisibleTab(chrome, tab.windowId);
    const after = await chrome.tabs.get(tabId);
    if (!after.active || after.url !== tab.url) throw new Error('page_changed: Page changed during screenshot.');
    if (image.length > 700 * 1024) throw new Error('result_too_large: Screenshot exceeds 700 KiB.');
    return {image, url:tab.url};
  }
  if (action === 'eval') {
    // Static localhost access is not consent to agent-supplied JavaScript.
    const hostGranted = hasExplicitHostGrant((await chrome.permissions.getAll()).origins, tab.url);
    if (!hostGranted) throw new Error('eval_not_granted: Grant this host (or all sites) explicitly before agent eval.');
    if (typeof args.script !== 'string' || args.script.length > 16384) throw new Error('invalid_script: Eval script must be at most 16 KiB.');
    const [reply] = await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:async (script) => {
      try {
        const value = await (0, eval)(script);
        const text = JSON.stringify(value ?? null);
        if (text.length > 16384) return {error:'result_too_large: Eval result exceeds 16 KiB.'};
        return {value:JSON.parse(text)};
      } catch (error) { return {error:String(error).slice(0,2048)}; }
    },args:[args.script]});
    if (reply?.result?.error) throw new Error(reply.result.error);
    return {value:reply?.result?.value ?? null};
  }
  throw new Error('unknown_command: Unsupported browser command.');
}

// Both agent screenshots and pick crops use Chrome's capture authority.
export async function captureVisibleTab(chrome, windowId) {
  try { return await chrome.tabs.captureVisibleTab(windowId, {format:'png'}); }
  catch (error) {
    if (error.message === "Either the '<all_urls>' or 'activeTab' permission is required.") {
      throw new Error('capture_permission_required: Enable “Let agents capture any site” in the Design Mode sidebar, or invoke the Design Mode browser action on this tab to grant temporary capture access.');
    }
    throw error;
  }
}

// Capture-only <all_urls> is not explicit consent to eval or remote error observation.
export function hasExplicitHostGrant(origins, raw) {
  const optional = (origins ?? []).filter((origin) => !['http://localhost/*','http://127.0.0.1/*','http://[::1]/*','https://localhost/*'].includes(origin));
  return optional.some((origin) => {
    const [scheme, pattern] = origin.split('://');
    const host = pattern?.split('/')[0];
    const url = new URL(raw);
    return (scheme === '*' || `${scheme}:` === url.protocol) && (host === '*' || host === url.hostname || (host?.startsWith('*.') && (url.hostname === host.slice(2) || url.hostname.endsWith(host.slice(1)))));
  });
}
