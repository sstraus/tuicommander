import QRCode from "qrcode";
import { type Component, createEffect, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { appLogger } from "../../stores/appLogger";
import { registerModal } from "../../stores/modalStack";
import { rpc } from "../../transport";
import { writeClipboard } from "../../utils/clipboard";
import d from "../shared/dialog.module.css";
import { Select } from "../ui/Select";
import s from "./RemoteQrDialog.module.css";

interface LocalIpEntry {
	ip: string;
	label: string;
}

/**
 * Full-size QR dialog for pairing a phone with Remote Mobile mode. Reuses the
 * same backend flow as Settings → Remote Access (`get_connect_url` builds the URL +
 * embeds the session token server-side; the raw token never reaches JS), but
 * renders the code large so it can be scanned at a glance from the palette.
 */
export const RemoteQrDialog: Component<{ onClose: () => void }> = (props) => {
	const [localIps, { refetch: refetchLocalIps }] = createResource(() => rpc<LocalIpEntry[]>("get_local_ips"));
	const availableIps = () => (localIps.error ? undefined : localIps());
	const [selectedIp, setSelectedIp] = createSignal<string | null>(null);
	const [serverEnabled, setServerEnabled] = createSignal<boolean | null>(null);
	const [connectUrl, setConnectUrl] = createSignal<string | null>(null);
	const [qrDataUrl, setQrDataUrl] = createSignal<string | null>(null);
	const [error, setError] = createSignal<string | null>(null);
	const [copied, setCopied] = createSignal(false);

	const activeIp = () => (localIps.error ? null : selectedIp() || availableIps()?.[0]?.ip || null);

	// Warn if the remote server is off — the QR will render but the phone can't
	// reach it until Remote Access is enabled.
	rpc<{ services: { server: { enabled: boolean } } }>("load_config")
		.then((c) => setServerEnabled(c.services.server.enabled))
		.catch(() => setServerEnabled(null));

	// Fetch the connect URL + render a large QR whenever the chosen IP changes.
	createEffect(() => {
		const ip = activeIp();
		if (!ip) {
			setConnectUrl(null);
			setQrDataUrl(null);
			return;
		}
		let cancelled = false;
		onCleanup(() => {
			cancelled = true;
		});
		setError(null);
		rpc<string>("get_connect_url", { ip })
			.then((url) => {
				if (cancelled) return;
				setConnectUrl(url);
				// Black-on-white for reliable scanning across phone cameras.
				return QRCode.toDataURL(url, { width: 320, margin: 2, color: { dark: "#000000", light: "#ffffff" } });
			})
			.then((dataUrl) => {
				if (!cancelled && dataUrl) setQrDataUrl(dataUrl);
			})
			.catch((e) => {
				if (cancelled) return;
				appLogger.warn("network", "Remote QR connect URL failed", e);
				setConnectUrl(null);
				setQrDataUrl(null);
				setError("Could not build a connection URL. Enable Remote Access in Settings → Remote Access.");
			});
	});

	// Escape-to-close is handled centrally (stores/modalStack): registering routes
	// Escape to props.onClose AND stops it reaching the terminal underneath.
	registerModal(props.onClose);

	const copyUrl = async () => {
		const url = connectUrl();
		if (!url) return;
		try {
			await writeClipboard(url);
			setCopied(true);
			setTimeout(() => setCopied(false), 1500);
		} catch (err) {
			appLogger.warn("network", "Remote QR copy URL failed", err);
		}
	};

	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div class={`${d.popover} ${s.dialog}`} onClick={(e) => e.stopPropagation()}>
				<div class={d.header}>
					<span class={d.headerIcon}>
						<QrIcon />
					</span>
					<div class={d.headerText}>
						<h4>Remote Mobile Connection</h4>
					</div>
				</div>
				<div class={d.body}>
					<Show when={serverEnabled() === false}>
						<p class={s.warning}>
							Remote Access is off — enable it in Settings → Remote Access so the phone can connect.
						</p>
					</Show>

					<Show when={!localIps.error}>
						<div class={s.qrCard}>
							<Show
								when={qrDataUrl()}
								fallback={
									<div class={s.placeholder}>
										{error() ?? (localIps.loading || activeIp() ? "Generating QR…" : "Not available")}
									</div>
								}
							>
								<img
									class={s.qr}
									src={qrDataUrl()!}
									alt="QR code for remote mobile connection"
									width={320}
									height={320}
								/>
							</Show>
						</div>
					</Show>
					<Show when={localIps.error}>
						<div class={s.discoveryFailure}>
							<p role="alert">Local IP discovery failed: {String(localIps.error)}</p>
							<div class={`${d.actions} ${s.retryActions}`}>
								<button
									type="button"
									class={d.cancelBtn}
									onClick={() => void Promise.resolve(refetchLocalIps()).catch(() => {})}
								>
									Retry
								</button>
							</div>
						</div>
					</Show>
					<Show when={(availableIps()?.length ?? 0) > 1}>
						<label class={s.ipRow}>
							<span>Network</span>
							<Select value={activeIp() ?? ""} onChange={(e) => setSelectedIp(e.currentTarget.value)}>
								<For each={availableIps()}>
									{(entry) => (
										<option value={entry.ip}>
											{entry.label} — {entry.ip}
										</option>
									)}
								</For>
							</Select>
						</label>
					</Show>

					<Show when={connectUrl()}>
						<button type="button" class={s.url} onClick={copyUrl} title="Copy connection URL">
							{copied() ? "Copied to clipboard" : connectUrl()}
						</button>
					</Show>
				</div>
			</div>
		</div>
	);
};

const QrIcon = () => (
	<svg
		width="18"
		height="18"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="2"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
	>
		<rect x="3" y="3" width="7" height="7" rx="1" />
		<rect x="14" y="3" width="7" height="7" rx="1" />
		<rect x="3" y="14" width="7" height="7" rx="1" />
		<line x1="14" y1="14" x2="14" y2="17" />
		<line x1="14" y1="21" x2="17" y2="21" />
		<line x1="17" y1="14" x2="21" y2="14" />
		<line x1="21" y1="17" x2="21" y2="21" />
		<line x1="17" y1="17" x2="17" y2="17.01" />
	</svg>
);
