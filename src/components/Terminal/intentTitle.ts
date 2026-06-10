/**
 * Decide whether an agent's `intent:` title should overwrite the tab name.
 *
 * A user-renamed tab (`nameIsCustom`) must never be clobbered by an agent
 * intent title, mirroring the OSC 0/2 title guard. Both the global setting
 * and the per-agent override must allow it.
 */
export function shouldApplyIntentTitle(opts: {
	title: string | null | undefined;
	globalEnabled: boolean;
	perAgentEnabled: boolean;
	nameIsCustom: boolean;
}): boolean {
	return Boolean(opts.title) && opts.globalEnabled && opts.perAgentEnabled && !opts.nameIsCustom;
}
