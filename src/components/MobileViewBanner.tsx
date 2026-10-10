import { createSignal, Show } from "solid-js";
import { isTauri } from "../transport";
import { isTabletUA, setDesktopUI } from "../utils/tabletRouting";

const DISMISS_KEY = "tuic-mobile-banner-dismissed";

/** Detect mobile user agent (phones only, not tablets) */
function isMobileUA(): boolean {
	if (typeof navigator === "undefined") return false;
	return /iPhone|Android.*Mobile|webOS|iPod/i.test(navigator.userAgent);
}

/**
 * Banner shown in the desktop UI on phones and tablets.
 * Suggests switching to /mobile. Dismissible with localStorage persistence.
 * Only renders in browser mode (never inside Tauri webview).
 */
export function MobileViewBanner() {
	const tablet = isTabletUA(navigator.userAgent, navigator.maxTouchPoints);
	const shouldShow = !isTauri() && (tablet || (isMobileUA() && !localStorage.getItem(DISMISS_KEY)));
	const [visible, setVisible] = createSignal(shouldShow);

	const dismiss = () => {
		setVisible(false);
		localStorage.setItem(DISMISS_KEY, "1");
	};

	return (
		<Show when={visible()}>
			<div
				style={{
					display: "flex",
					"align-items": "center",
					"justify-content": "center",
					gap: "12px",
					padding: "8px 16px",
					background: "var(--bg-tertiary)",
					"border-bottom": "1px solid var(--border)",
					"font-size": "13px",
					color: "var(--fg-secondary)",
					"flex-shrink": "0",
				}}
			>
				<span>Mobile view available for a better experience on this device.</span>
				<a
					href="/mobile"
					onClick={() => setDesktopUI(false)}
					style={{
						color: "var(--accent)",
						"text-decoration": "none",
						"font-weight": "500",
					}}
				>
					Switch
				</a>
				<Show when={!tablet}>
					<button
						onClick={dismiss}
						aria-label="Dismiss mobile banner"
						style={{
							background: "none",
							border: "none",
							color: "var(--fg-muted)",
							cursor: "pointer",
							padding: "2px 6px",
							"font-size": "16px",
							"line-height": "1",
						}}
					>
						&times;
					</button>
				</Show>
			</div>
		</Show>
	);
}
