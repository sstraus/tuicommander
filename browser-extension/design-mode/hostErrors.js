import {consentedRemoteHosts} from './consent.js';
// Dynamic scripts cover only consented remote hosts; static localhost scripts remain unchanged.
// Chrome's all-sites capture grant alone does not opt a host into remote error injection.
export async function syncHostErrors(chrome) {
  const matches = (await consentedRemoteHosts(chrome.storage.session)).map((host) => `*://${host}/*`);
  const ids = ['tuic-granted-errors-main', 'tuic-granted-errors-bridge'];
  const existing = await chrome.scripting.getRegisteredContentScripts({ids});
  if (existing.length) await chrome.scripting.unregisterContentScripts({ids:existing.map((script) => script.id)});
  if (!matches.length) return;
  await chrome.scripting.registerContentScripts([
    {id:ids[0],matches,js:['errors.js'],world:'MAIN',runAt:'document_start',allFrames:true},
    {id:ids[1],matches,js:['errorsBridge.js'],world:'ISOLATED',runAt:'document_start',allFrames:true},
  ]);
}
