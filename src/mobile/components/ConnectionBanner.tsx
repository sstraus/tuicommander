import { Match, Switch } from "solid-js";
import { Button } from "../../components/ui/Button";
import { StatusBadge } from "../../components/ui/StatusBadge";
import styles from "./ConnectionBanner.module.css";

interface ConnectionBannerProps {
	/** Last poll failed, or the version check lost the server. */
	offline: boolean;
	/** The server refused this client (401/403). */
	authError: boolean;
	onRetry: () => void;
	error?: string | null;
	testing?: boolean;
}

/** Connection-test surface for a running app; mounted views retain their data and drafts. */
export function ConnectionBanner(props: ConnectionBannerProps) {
	return (
		<Switch>
			<Match when={props.authError}>
				<div class={styles.banner} role="alert">
					<span>Access refused — sign in again</span>
					<a class={styles.signIn} href={`/mobile/login?next=${encodeURIComponent(location.pathname)}`}>
						Sign in
					</a>
				</div>
			</Match>
			<Match when={props.offline}>
				<section class={styles.landing} data-connection-landing aria-label="Connection test">
					<h1>Server unreachable</h1>
					<p class={styles.server}>{location.origin}</p>
					<StatusBadge label={props.testing ? "Testing connection…" : "Unreachable"} variant="warning" />
					<p>Reconnecting automatically. Your views and drafts are retained.</p>
					<p class={styles.server}>
						Last error: <span>{props.error ?? "Could not connect to the server."}</span>
					</p>
					<Button onClick={props.onRetry}>Retry now</Button>
				</section>
			</Match>
		</Switch>
	);
}
