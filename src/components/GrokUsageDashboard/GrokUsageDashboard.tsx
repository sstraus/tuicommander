import { type Component, createEffect, createSignal, on, onCleanup, Show } from "solid-js";
import { type GrokUsageApiResponse, grokPeriodLabel } from "../../features/grokUsage";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import { providerQuotaStore } from "../../stores/providerQuota";
import s from "../ClaudeUsageDashboard/ClaudeUsageDashboard.module.css";

function rateClass(usedPercent: number): string {
	if (usedPercent >= 90) return s.rateCritical;
	if (usedPercent >= 70) return s.rateWarn;
	return s.rateOk;
}

export function formatMoney(value: number | null | undefined): string {
	return value === null || value === undefined || !Number.isFinite(value) ? "--" : value.toFixed(2);
}

export function formatPeriodEnd(value: string | null | undefined): string {
	if (!value) return "--";
	const date = new Date(value);
	return Number.isNaN(date.valueOf()) ? value : date.toLocaleString();
}

export const GrokUsageDashboard: Component<{ sessionId?: string | (() => string | undefined) }> = (props) => {
	const selectedSessionId = () => (typeof props.sessionId === "function" ? props.sessionId() : props.sessionId);
	const [hostUsage, setUsage] = createSignal<GrokUsageApiResponse | null>(null);
	const [error, setError] = createSignal<string | null>(null);
	const [loading, setLoading] = createSignal(true);

	const usage = () => {
		const sessionId = selectedSessionId();
		if (!sessionId) return hostUsage();
		const detail = providerQuotaStore.viewForAgent(sessionId, "grok")?.snapshot?.detail;
		return detail?.provider === "grok" ? (detail.response as GrokUsageApiResponse) : null;
	};
	createEffect(
		on(selectedSessionId, (sessionId) => {
			let disposed = false;
			setUsage(null);
			setError(null);
			setLoading(true);
			const refresh = async () => {
				try {
					if (sessionId) {
						await providerQuotaStore.start();
					} else {
						const response = await invoke<GrokUsageApiResponse>("get_grok_usage_api");
						if (!disposed) setUsage(response);
					}
					if (!disposed) setError(null);
				} catch (reason) {
					if (disposed) return;
					const message = String(reason);
					setError(message);
					appLogger.warn("network", "Grok usage fetch failed", message);
				} finally {
					if (!disposed) setLoading(false);
				}
			};
			void refresh();
			const timer = setInterval(() => void refresh(), 5 * 60 * 1000);
			onCleanup(() => {
				disposed = true;
				clearInterval(timer);
			});
		}),
	);

	return (
		<div class={s.dashboard}>
			<div class={s.header}>
				<span class={s.title}>Grok Usage Dashboard</span>
				<Show when={usage()?.subscription_tier}>{(tier) => <span class={s.sectionHint}>{tier()}</span>}</Show>
			</div>

			<Show when={!loading()} fallback={<div class={s.section}>Loading…</div>}>
				<Show when={usage()} fallback={<div class={s.errorState}>{error() ?? "No billing data available."}</div>}>
					{(api) => {
						const used = () => api().config.credit_usage_percent;
						return (
							<>
								<div class={s.section}>
									<div class={s.sectionTitle}>Usage</div>
									<Show
										when={used() !== null}
										fallback={<div class={s.rateLimitHint}>Provider did not report a usage percentage.</div>}
									>
										<div class={s.rateGrid}>
											<div class={s.rateCard}>
												<div class={s.rateLabel}>
													<span>{grokPeriodLabel(api().config.current_period?.period_type)}</span>
													<span class={s.rateValue}>{Math.round(used() ?? 0)}%</span>
												</div>
												<div class={s.rateBar}>
													<div
														class={`${s.rateFill} ${rateClass(used() ?? 0)}`}
														style={{ transform: `scaleX(${Math.min(100, used() ?? 0) / 100})` }}
													/>
												</div>
												<span class={s.rateReset}>Period ends {formatPeriodEnd(api().config.current_period?.end)}</span>
											</div>
										</div>
									</Show>
								</div>

								<div class={s.section}>
									<div class={s.sectionTitle}>Billing</div>
									<div class={s.insightsGrid}>
										<div class={s.insightCard}>
											<span class={s.insightLabel}>On-demand used</span>
											<span class={s.insightValue}>{formatMoney(api().config.on_demand_used?.val)}</span>
										</div>
										<div class={s.insightCard}>
											<span class={s.insightLabel}>On-demand cap</span>
											<span class={s.insightValue}>{formatMoney(api().config.on_demand_cap?.val)}</span>
										</div>
										<div class={s.insightCard}>
											<span class={s.insightLabel}>Prepaid balance</span>
											<span class={s.insightValue}>{formatMoney(api().config.prepaid_balance?.val)}</span>
										</div>
									</div>
								</div>
							</>
						);
					}}
				</Show>
			</Show>
		</div>
	);
};
