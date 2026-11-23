import { type Component, Show } from "solid-js";
import { clockTime } from "../../utils/clockTime";
import s from "./GatewayNotice.module.css";

export interface GatewayNoticeProps {
	/** The gateway's refusal: shows when the quota resets and nothing retries before then. */
	limit?: { until: number } | null;
	/** When the rows on screen were last loaded; shown only while they are stale. */
	cachedAt?: number | null;
	stale?: boolean;
}

/** The one place a rate limit or a stale source is told to the user. */
export const GatewayNotice: Component<GatewayNoticeProps> = (props) => (
	<>
		<Show when={props.limit}>
			{(limit) => <span class={s.limit}>Rate limited · resets {clockTime(limit().until)}</span>}
		</Show>
		<Show when={props.stale && props.cachedAt}>
			{(at) => (
				<span class={s.cached} data-tone="muted">
					cached {clockTime(at())}
				</span>
			)}
		</Show>
	</>
);
