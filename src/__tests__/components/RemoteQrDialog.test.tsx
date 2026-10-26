import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";

const url = "http://192.168.1.42:9876/connect?token=" + "a".repeat(100);
vi.mock("../../transport", () => ({
	rpc: vi.fn(async (command: string) =>
		command === "get_local_ips"
			? [{ ip: "192.168.1.42", label: "LAN" }]
			: command === "get_connect_url"
				? url
				: { services: { server: { enabled: true } } },
	),
}));
vi.mock("qrcode", () => ({ default: { toDataURL: vi.fn().mockResolvedValue("data:image/png;base64,AA==") } }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: vi.fn().mockResolvedValue(undefined) }));

import { RemoteQrDialog } from "../../components/RemoteQrDialog/RemoteQrDialog";
import { writeClipboard } from "../../utils/clipboard";

afterEach(() => {
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.clearAllMocks();
});
// Catches: splitting the URL/action truncates readable text or copies the action label instead of the full URL.
it("keeps the full URL separate and copies it through the named action", async () => {
	vi.useFakeTimers();
	const view = render(() => <RemoteQrDialog onClose={() => {}} />);
	await vi.advanceTimersByTimeAsync(0);
	const text = view.getByText(url);
	expect(text.tagName).toBe("CODE");
	expect(text.closest("button")).toBeNull();
	const copy = view.getByRole("button", { name: "Copy connection URL" });
	expect(copy.textContent).toBe("Copy URL");
	fireEvent.click(copy);
	await vi.advanceTimersByTimeAsync(0);
	expect(writeClipboard).toHaveBeenCalledExactlyOnceWith(url);
	expect(copy.textContent).toBe("Copied");
	expect(text.textContent).toBe(url);
	await vi.advanceTimersByTimeAsync(1500);
	expect(copy.textContent).toBe("Copy URL");
});
