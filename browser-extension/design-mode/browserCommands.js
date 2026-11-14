import {captureOptions, encodeCaptureWithin, MAX_SCREENSHOT_BYTES} from './captureEncoding.js';
import {hasHostConsent, isLocalHost} from './consent.js';
const CONSENT = 'click Allow agents on this host in the Design Mode sidebar';
// The host refuses browser replies over 768 KiB and pages eval evidence itself.
const MAX_EVAL_RESULT = 640 * 1024;
// Chrome owns host permissions, capture authority and reviewed-tab identity. No debugger/CDP access.
// TUIC adds one owner consent per armed session and host on top of them.
export async function browserCommand(command, chrome, tabId, errors) {
  if (!tabId) throw new Error('no_reviewed_tab: Open the Design Mode sidebar on a page first.');
  const tab = await chrome.tabs.get(tabId);
  const {action, args = {}} = command;
  // Static localhost access covers inspection; agent JavaScript and every remote host need consent.
  const allowed = async (raw, script = false) => {
    const url = new URL(raw);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('host_denied: Use an HTTP(S) URL without credentials.');
    if ((script || !isLocalHost(raw)) && !(await hasHostConsent(chrome.storage.session, command.sessionId, raw))) {
      throw new Error(script ? `eval_not_granted: grant needed: ${CONSENT}.` : `host_denied: ${CONSENT} first.`);
    }
    return url;
  };
  if (action === 'navigate') {
    await allowed(args.url);
    await chrome.tabs.update(tabId, {url:args.url});
    return {url:args.url};
  }
  const page = await allowed(tab.url, action === 'eval');
  if (action === 'reload') { await chrome.tabs.reload(tabId); return {reloaded:true}; }
  if (action === 'errors') return {errors:await errors(tabId)};
  if (action === 'screenshot') {
    if (!tab.active) throw new Error('tab_not_visible: The reviewed tab must be visible.');
    const options = captureOptions(args);
    const data = await captureVisibleTab(chrome, tab.windowId);
    const after = await chrome.tabs.get(tabId);
    if (!after.active || after.url !== tab.url) throw new Error('page_changed: Page changed during screenshot.');
    const capture = await encodeCaptureWithin(await createImageBitmap(await (await fetch(data)).blob()), options, MAX_SCREENSHOT_BYTES);
    return {...capture, url:tab.url};
  }
  if (action === 'eval') {
    if (typeof args.script !== 'string') throw new Error('invalid_script: Eval script must be a string.');
    const [reply] = await chrome.scripting.executeScript({target:{tabId},world:'MAIN',func:async (script, host, limit) => {
      // Consent is per host: a document that navigated away since the check is not evaluated.
      if (location.hostname !== host) return {error:'page_changed: Page changed before eval.'};
      try {
        const value = await (0, eval)(script);
        const text = JSON.stringify(value ?? null);
        if (new TextEncoder().encode(text).length > limit) return {error:`result_too_large: Eval result exceeds ${limit / 1024} KiB.`};
        return {value:JSON.parse(text)};
      } catch (error) { return {error:String(error).slice(0,2048)}; }
    },args:[args.script, page.hostname, MAX_EVAL_RESULT]});
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
      throw new Error('capture_permission_required: Click “Allow agents on this host” in the Design Mode sidebar, or invoke the Design Mode browser action on this tab to grant temporary capture access.');
    }
    throw error;
  }
}
