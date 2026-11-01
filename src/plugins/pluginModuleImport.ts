/** Native module transport, separate from the loader's recovery policy. */
export function importPluginModule(url: string): Promise<unknown> {
	return import(/* @vite-ignore */ url);
}
