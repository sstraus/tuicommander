import { invoke } from "../invoke";

// All frontend app-config writers share this queue. Each save carries its loaded
// base and edited document; serializing local updates keeps their order.
let configWriteTail: Promise<void> = Promise.resolve();

export function runSerializedConfigWrite<T>(write: () => Promise<T>): Promise<T> {
	const operation = configWriteTail.then(write);
	configWriteTail = operation.then(
		() => undefined,
		() => undefined,
	);
	return operation;
}

export function updateAppConfig<T extends object>(mutate: (config: T) => void): Promise<T> {
	return runSerializedConfigWrite(async () => {
		const loaded = await invoke<T>("load_config");
		const config = loaded ?? ({} as T);
		const base = structuredClone(config);
		mutate(config);
		await invoke("save_config", { base, config });
		return config;
	});
}
