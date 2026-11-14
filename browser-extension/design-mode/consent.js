// TUIC's own agent consent: one owner choice per armed session and host.
// Chrome permissions stay separate prerequisites that Chrome itself enforces.
const KEY = 'hostConsents';
const LIMIT = 64;
const LOCAL = ['localhost', '127.0.0.1', '[::1]'];
const entry = (sessionId, raw) => JSON.stringify([sessionId, new URL(raw).hostname]);
async function consents(storage) { return (await storage.get(KEY))[KEY] ?? []; }

export const isLocalHost = (raw) => LOCAL.includes(new URL(raw).hostname);

export async function hasHostConsent(storage, sessionId, raw) {
  return Boolean(sessionId) && (await consents(storage)).includes(entry(sessionId, raw));
}

// Only the sidebar calls this, after the owner's click; agents and pages have no path here.
export async function grantHostConsent(storage, sessionId, raw) {
  if (!sessionId) throw new Error('not_armed: Start Design Mode from TUICommander before allowing agents.');
  const key = entry(sessionId, raw);
  await storage.set({[KEY]: [...(await consents(storage)).filter((value) => value !== key), key].slice(-LIMIT)});
}

// Remote hostnames consented in any session, for page-error observation.
export async function consentedRemoteHosts(storage) {
  const hosts = (await consents(storage)).map((value) => JSON.parse(value)[1]);
  return [...new Set(hosts)].filter((host) => !LOCAL.includes(host));
}
