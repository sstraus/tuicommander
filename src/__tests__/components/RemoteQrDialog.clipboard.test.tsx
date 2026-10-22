import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
	rpc: vi.fn(),
	writeClipboard: vi.fn(),
}));

vi.mock("../../transport", () => ({ rpc: h.rpc }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: h.writeClipboard }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));
vi.mock("qrcode", () => ({ default: { toDataURL: vi.fn().mockResolvedValue("data:image/png;base64,fakeqr") } }));

import { RemoteQrDialog } from "../../components/RemoteQrDialog/RemoteQrDialog";
import { appLogger } from "../../stores/appLogger";

describe("RemoteQrDialog copy URL clipboard handling", () => {
	beforeEach(() => {
		h.writeClipboard.mockReset().mockResolvedValue(undefined);
		vi.mocked(appLogger.warn).mockClear();
		h.rpc.mockReset().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
			switch (cmd) {
				case "get_local_ips":
					return Promise.resolve([{ ip: "192.168.1.10", label: "Wi-Fi" }]);
				case "load_config":
					return Promise.resolve({ services: { server: { enabled: true } } });
				case "get_connect_url":
					return Promise.resolve(`https://example.test/connect?ip=${args?.ip}`);
				default:
					return Promise.reject(new Error(`unmocked rpc: ${cmd}`));
			}
		});
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("shows 'Copied to clipboard' after a successful write", async () => {
		vi.useFakeTimers();
		const { getByTitle, getByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await vi.waitFor(() => expect(getByTitle("Copy connection URL")).toBeTruthy());

		fireEvent.click(getByTitle("Copy connection URL"));

		await vi.waitFor(() => expect(getByText("Copied to clipboard")).toBeTruthy());
		await vi.advanceTimersByTimeAsync(1500);
		expect(h.writeClipboard).toHaveBeenCalledWith("https://example.test/connect?ip=192.168.1.10");
	});

	it("logs a denied write, does not throw, and never shows a false 'Copied' state", async () => {
		const unhandled = vi.fn();
		process.on("unhandledRejection", unhandled);
		try {
			const err = new DOMException("Write permission denied.", "NotAllowedError");
			h.writeClipboard.mockRejectedValue(err);
			const { findByAltText, getByTitle, queryByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
			await findByAltText("QR code for remote mobile connection");

			fireEvent.click(getByTitle("Copy connection URL"));

			await waitFor(() => expect(appLogger.warn).toHaveBeenCalledWith("network", "Remote QR copy URL failed", err));
			await new Promise((r) => setTimeout(r, 20));
			expect(queryByText("Copied to clipboard")).toBeNull();
			expect(unhandled).not.toHaveBeenCalled();
		} finally {
			process.off("unhandledRejection", unhandled);
		}
	});
});
