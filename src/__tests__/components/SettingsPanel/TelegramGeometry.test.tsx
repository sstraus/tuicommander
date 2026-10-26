import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("../../../invoke", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import { TelegramTab } from "../../../components/SettingsPanel/tabs/TelegramTab";

afterEach(() => {
	cleanup();
	invoke.mockReset();
});

// Catches: migrated token controls submit an empty token, retain it after submission or allow duplicate busy actions.
it("retains token clearing and busy guards through shared controls", async () => {
	let finish!: (value: object) => void;
	invoke.mockImplementation(async (command: string) => {
		if (command === "telegram_settings")
			return {
				enabled: true,
				token_set: false,
				bot_alias: "",
				registered_agent_name: null,
				chats: ["123"],
				pairing_pending: false,
				recent_events: [],
				connected: true,
				polling_owner: "this_app",
				last_error: null,
				last_message_time: null,
			};
		if (command === "telegram_setup")
			return new Promise((resolve) => {
				finish = resolve;
			});
		throw new Error(`Unexpected test command: ${command}`);
	});
	render(() => <TelegramTab />);
	const save = await screen.findByRole("button", { name: "Save and check bot" });
	expect(save).toBeDisabled();
	expect(screen.getByRole("button", { name: "Add chat ID" })).toBeDisabled();
	const token = screen.getByLabelText("Bot token") as HTMLInputElement;
	fireEvent.input(token, { target: { value: "fixture-token" } });
	fireEvent.click(save);
	expect(token.value).toBe("");
	expect(save).toBeDisabled();
	expect(screen.getByRole("button", { name: "Link chat" })).toBeDisabled();
	expect(invoke).toHaveBeenCalledWith("telegram_setup", { change: { action: "token", token: "fixture-token" } });
	finish({});
	await waitFor(() => expect(screen.getByRole("button", { name: "Link chat" })).toBeEnabled());
});
