import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

// Recorded verbatim from tauri-plugin-notification 2.7.0 src/init-iife.js.
const initialization = readFileSync("src/__tests__/fixtures/notification-init-2.7.0.txt", "utf8");
const capability = JSON.parse(readFileSync("src-tauri/capabilities/default.json", "utf8"));

// Catches: an unhandled permission rejection before app startup.
it("notification_boot_probe_is_authorized_2568", async () => {
	const calls: string[] = [];
	const host = {
		__TAURI_INTERNALS__: {
			invoke: async (command: string) => {
				calls.push(command);
				return true;
			},
		},
	};
	new Function("window", initialization.replaceAll("__TEMPLATE_windows__", "false"))(host);
	await Promise.resolve();
	await Promise.resolve();
	expect(calls).toEqual(["plugin:notification|is_permission_granted"]);
	for (const command of calls) {
		expect(capability.permissions).toContain(`notification:allow-${command.split("|")[1].replaceAll("_", "-")}`);
	}
});

it("retains request and delivery permissions — catches fixing the probe by replacing existing notification grants", () => {
	expect(capability.permissions).toEqual(
		expect.arrayContaining(["notification:allow-request-permission", "notification:allow-notify"]),
	);
});

it("covers all app-owned windows — catches detached panels losing notification initialization permission", () => {
	expect(capability.windows).toEqual(expect.arrayContaining(["main", "floating-*", "panel-*"]));
});
