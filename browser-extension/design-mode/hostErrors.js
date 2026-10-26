// Dynamic scripts cover only granted remote origins; static localhost scripts remain unchanged.
const LOCAL = new Set(['http://localhost/*','http://127.0.0.1/*','http://[::1]/*','https://localhost/*']);
export async function syncHostErrors(chrome) {
  const {origins = []} = await chrome.permissions.getAll();
  // The capture-only all-URLs option does not opt into remote error injection.
  const matches = origins.filter((origin) => origin !== '<all_urls>' && !LOCAL.has(origin));
  const ids = ['tuic-granted-errors-main', 'tuic-granted-errors-bridge'];
  const existing = await chrome.scripting.getRegisteredContentScripts({ids});
  if (existing.length) await chrome.scripting.unregisterContentScripts({ids:existing.map((script) => script.id)});
  if (!matches.length) return;
  await chrome.scripting.registerContentScripts([
    {id:ids[0],matches,js:['errors.js'],world:'MAIN',runAt:'document_start',allFrames:true},
    {id:ids[1],matches,js:['errorsBridge.js'],world:'ISOLATED',runAt:'document_start',allFrames:true},
  ]);
}
