import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
	rpc: vi.fn(),
	writeClipboard: vi.fn(),
	toDataURL: vi.fn(),
}));

vi.mock("../../transport", () => ({ rpc: h.rpc }));
vi.mock("../../utils/clipboard", () => ({ writeClipboard: h.writeClipboard }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));
vi.mock("qrcode", () => ({ default: { toDataURL: h.toDataURL } }));

import { RemoteQrDialog } from "../../components/RemoteQrDialog/RemoteQrDialog";
import { appLogger } from "../../stores/appLogger";

const LOCAL_IPS = [
	{ ip: "192.168.1.10", label: "Wi-Fi" },
	{ ip: "10.0.0.5", label: "Ethernet" },
];

function mockRpc(overrides: Partial<Record<string, unknown>> = {}) {
	h.rpc.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
		switch (cmd) {
			case "get_local_ips":
				return Promise.resolve("localIps" in overrides ? overrides.localIps : LOCAL_IPS);
			case "load_config":
				return Promise.resolve(
					"loadConfig" in overrides ? overrides.loadConfig : { services: { server: { enabled: true } } },
				);
			case "get_connect_url":
				if ("connectUrlError" in overrides) return Promise.reject(overrides.connectUrlError);
				return Promise.resolve(
					"connectUrl" in overrides ? overrides.connectUrl : `https://example.test/connect?ip=${args?.ip}`,
				);
			default:
				return Promise.reject(new Error(`unmocked rpc: ${cmd}`));
		}
	});
}

describe("RemoteQrDialog", () => {
	beforeEach(() => {
		h.rpc.mockReset();
		h.writeClipboard.mockReset().mockResolvedValue(undefined);
		h.toDataURL.mockReset().mockResolvedValue("data:image/png;base64,fakeqr");
		vi.mocked(appLogger.warn).mockClear();
		mockRpc();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("shows a generating placeholder while the connect URL request is still pending", async () => {
		// IPs resolve, so the effect issues get_connect_url; that request never settles.
		h.rpc.mockImplementation((cmd: string) => {
			if (cmd === "get_local_ips") return Promise.resolve(LOCAL_IPS);
			if (cmd === "load_config") return Promise.resolve({ services: { server: { enabled: true } } });
			return new Promise(() => {});
		});
		const { getByText, queryByAltText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);

		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("get_connect_url", { ip: "192.168.1.10" }));
		expect(getByText("Generating QR…")).toBeTruthy();
		expect(queryByAltText("QR code for remote mobile connection")).toBeNull();
		expect(h.toDataURL).not.toHaveBeenCalled();
	});

	it("says the QR is not available and never requests a connect URL when there are no local IPs", async () => {
		mockRpc({ localIps: [] });
		const { findByText, queryByAltText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);

		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("get_local_ips"));
		await waitFor(() => expect(h.rpc).toHaveBeenCalledWith("load_config"));
		// Once discovery settles with no addresses the placeholder stops claiming it is generating.
		expect(await findByText("Not available")).toBeTruthy();
		expect(queryByAltText("QR code for remote mobile connection")).toBeNull();
		expect(h.rpc).not.toHaveBeenCalledWith("get_connect_url", expect.anything());
	});

	it("renders the QR image and connect URL once they resolve", async () => {
		const { findByAltText, getByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);

		const img = (await findByAltText("QR code for remote mobile connection")) as HTMLImageElement;
		expect(img.src).toBe("data:image/png;base64,fakeqr");
		await waitFor(() => expect(getByText("https://example.test/connect?ip=192.168.1.10")).toBeTruthy());
		expect(h.toDataURL).toHaveBeenCalledWith(
			"https://example.test/connect?ip=192.168.1.10",
			expect.objectContaining({ width: 320, margin: 2, color: { dark: "#000000", light: "#ffffff" } }),
		);
	});

	it("shows a warning when Remote Access is disabled", async () => {
		mockRpc({ loadConfig: { services: { server: { enabled: false } } } });
		const { findByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);

		expect(
			await findByText("Remote Access is off — enable it in Settings → Remote Access so the phone can connect."),
		).toBeTruthy();
	});

	it("does not show the disabled warning once Remote Access is confirmed enabled", async () => {
		const { findByAltText, queryByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await findByAltText("QR code for remote mobile connection");

		expect(queryByText(/Remote Access is off/)).toBeNull();
	});

	it("shows an error placeholder and logs when the connect URL fetch fails", async () => {
		const err = new Error("no route");
		mockRpc({ connectUrlError: err });
		const { findByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);

		expect(
			await findByText("Could not build a connection URL. Enable Remote Access in Settings → Remote Access."),
		).toBeTruthy();
		expect(appLogger.warn).toHaveBeenCalledWith("network", "Remote QR connect URL failed", err);
	});

	it("does not render the copy-URL button while there is no connect URL", async () => {
		mockRpc({ connectUrlError: new Error("no route") });
		const { findByText, queryByTitle } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await findByText("Could not build a connection URL. Enable Remote Access in Settings → Remote Access.");

		expect(queryByTitle("Copy connection URL")).toBeNull();
	});

	it("does not show the network selector when there is only one local IP", async () => {
		mockRpc({ localIps: [LOCAL_IPS[0]] });
		const { findByAltText, queryByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await findByAltText("QR code for remote mobile connection");

		expect(queryByText("Network")).toBeNull();
	});

	it("shows a network selector with all local IPs when there is more than one", async () => {
		const { findByAltText, getByText, container } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await findByAltText("QR code for remote mobile connection");

		expect(getByText("Network")).toBeTruthy();
		const options = container.querySelectorAll("option");
		expect(options.length).toBe(2);
		expect(options[0].textContent).toBe("Wi-Fi — 192.168.1.10");
		expect(options[1].textContent).toBe("Ethernet — 10.0.0.5");
	});

	it("re-fetches the connect URL and QR code when the selected IP changes", async () => {
		const { findByAltText, container, getByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await findByAltText("QR code for remote mobile connection");
		h.rpc.mockClear();
		h.toDataURL.mockClear();
		h.toDataURL.mockImplementation((url: string) => Promise.resolve(`data:image/png;base64,qr-for-${url}`));

		const select = container.querySelector("select") as HTMLSelectElement;
		fireEvent.change(select, { target: { value: "10.0.0.5" } });

		await waitFor(() => expect(getByText("https://example.test/connect?ip=10.0.0.5")).toBeTruthy());
		expect(h.rpc).toHaveBeenCalledWith("get_connect_url", { ip: "10.0.0.5" });
		expect(h.toDataURL).toHaveBeenCalledWith("https://example.test/connect?ip=10.0.0.5", expect.any(Object));
		await waitFor(() =>
			expect((container.querySelector("img") as HTMLImageElement).src).toBe(
				"data:image/png;base64,qr-for-https://example.test/connect?ip=10.0.0.5",
			),
		);
	});

	it("copies the connect URL to the clipboard and shows a confirmation", async () => {
		vi.useFakeTimers();
		const { getByTitle, getByText, queryByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await vi.waitFor(() => expect(getByTitle("Copy connection URL")).toBeTruthy());

		fireEvent.click(getByTitle("Copy connection URL"));

		await vi.waitFor(() => expect(getByText("Copied to clipboard")).toBeTruthy());
		expect(h.writeClipboard).toHaveBeenCalledWith("https://example.test/connect?ip=192.168.1.10");

		await vi.advanceTimersByTimeAsync(1500);
		expect(queryByText("Copied to clipboard")).toBeNull();
		expect(getByText("https://example.test/connect?ip=192.168.1.10")).toBeTruthy();
	});

	it("does nothing when the copy button is clicked before a connect URL exists", async () => {
		mockRpc({ connectUrlError: new Error("no route") });
		const { findByText } = render(() => <RemoteQrDialog onClose={vi.fn()} />);
		await findByText("Could not build a connection URL. Enable Remote Access in Settings → Remote Access.");

		// No button is rendered in this state (covered above); writeClipboard must never fire.
		expect(h.writeClipboard).not.toHaveBeenCalled();
	});

	it("closes when the overlay is clicked", async () => {
		const onClose = vi.fn();
		const { findByAltText, container } = render(() => <RemoteQrDialog onClose={onClose} />);
		await findByAltText("QR code for remote mobile connection");

		fireEvent.click(container.firstElementChild as HTMLElement);

		expect(onClose).toHaveBeenCalled();
	});

	it("does not close when the dialog popover itself is clicked", async () => {
		const onClose = vi.fn();
		const { findByAltText, container } = render(() => <RemoteQrDialog onClose={onClose} />);
		await findByAltText("QR code for remote mobile connection");

		const popover = container.querySelector("[class*='dialog']") as HTMLElement;
		fireEvent.click(popover);

		expect(onClose).not.toHaveBeenCalled();
	});

	it("closes on Escape via the shared modal stack", async () => {
		const onClose = vi.fn();
		const { findByAltText } = render(() => <RemoteQrDialog onClose={onClose} />);
		await findByAltText("QR code for remote mobile connection");

		fireEvent.keyDown(document, { key: "Escape" });

		expect(onClose).toHaveBeenCalled();
	});
});
