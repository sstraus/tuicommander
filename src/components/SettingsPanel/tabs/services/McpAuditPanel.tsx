import { type Component, createMemo, createSignal, For, onMount, Show } from "solid-js";
import { locale, t } from "../../../../i18n";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { Button } from "../../../ui/Button";
import { Select } from "../../../ui/Select";
import { type BadgeVariant, StatusBadge } from "../../../ui/StatusBadge";
import s from "../../Settings.module.css";
import css from "./McpAuditPanel.module.css";

/** Body-free row of `GET /mcp/audit`; only these fields are ever read. */
interface AuditRow {
	call_id: string;
	operation: string;
	target_digest: string | null;
	target?: { kind: string; id: string; name: string | null } | null;
	started_ms: number;
	response_text_bytes: number | null;
	effect: string;
	outcome: string;
}

interface AuditSnapshot {
	enabled: boolean;
	records: AuditRow[];
	retained?: number;
}

const OUTCOMES: Record<string, { label: () => string; variant?: BadgeVariant }> = {
	replied: { label: () => t("mcpAudit.outcome.replied", "Replied") },
	error: { label: () => t("mcpAudit.outcome.error", "Error"), variant: "error" },
	cancelled: { label: () => t("mcpAudit.outcome.cancelled", "Cancelled"), variant: "warning" },
};

// "Unknown" is never styled as success: the server did not observe the effect.
const EFFECTS: Record<string, { label: () => string; hint: () => string; variant?: BadgeVariant }> = {
	completed: {
		label: () => t("mcpAudit.effect.completed", "Completed"),
		variant: "success",
		hint: () =>
			t(
				"mcpAudit.effect.completedHint",
				"The handler returned successfully. Queued work may still run; client delivery is not confirmed.",
			),
	},
	read_only: {
		label: () => t("mcpAudit.effect.readOnly", "Read-only"),
		hint: () =>
			t("mcpAudit.effect.readOnlyHint", "The read-only handler returned successfully without a mutating operation."),
	},
	failed: {
		label: () => t("mcpAudit.effect.failed", "Failed"),
		variant: "error",
		hint: () =>
			t(
				"mcpAudit.effect.failedHint",
				"The handler returned an application error. This does not prove rollback of partial changes.",
			),
	},
	not_started: {
		label: () => t("mcpAudit.effect.notStarted", "Not started"),
		hint: () => t("mcpAudit.effect.notStartedHint", "The request was refused before the operation started."),
	},
	unknown: {
		label: () => t("mcpAudit.effect.unknown", "Unknown"),
		variant: "warning",
		hint: () =>
			t(
				"mcpAudit.effect.unknownHint",
				"Completion is uncertain: the call was cancelled, the handler was interrupted, or an upstream reply was lost. Partial effects may still have occurred.",
			),
	},
};

const DIGEST_CHARS = 8;

/** MCP call audit: newest tools/call observations; fetched on open and on demand only. */
export const McpAuditPanel: Component = () => {
	const [snapshot, setSnapshot] = createSignal<AuditSnapshot | null>(null);
	const [failed, setFailed] = createSignal(false);
	const [filter, setFilter] = createSignal("all");

	const refresh = async () => {
		try {
			setSnapshot(await rpc<AuditSnapshot>("get_mcp_audit"));
			setFailed(false);
		} catch (error) {
			appLogger.warn("config", "Failed to load MCP call audit", error);
			setFailed(true);
		}
	};
	onMount(refresh);

	const rows = createMemo(() =>
		(snapshot()?.records ?? []).filter((r) => filter() === "all" || r.outcome === filter()),
	);

	return (
		<div>
			<div class={css.toolbar}>
				<Select
					compact
					data-non-setting="Filters the audit rows on screen; not a preference"
					aria-label={t("mcpAudit.filter", "Filter by outcome")}
					value={filter()}
					onChange={(e) => setFilter(e.currentTarget.value)}
				>
					<option value="all">{t("mcpAudit.filter.all", "All outcomes")}</option>
					<For each={Object.entries(OUTCOMES)}>{([value, o]) => <option value={value}>{o.label()}</option>}</For>
				</Select>
				<span class={css.spacer} />
				<Button
					type="button"
					class={s.testBtn}
					data-non-setting="Reload the audit rows; not a preference"
					onClick={refresh}
				>
					{t("mcpAudit.refresh", "Refresh")}
				</Button>
			</div>
			<Show when={failed()}>
				<p class={css.empty} role="alert">
					{t("mcpAudit.failed", "Could not load the call audit.")}
				</p>
			</Show>
			<Show
				when={snapshot()?.enabled !== false}
				fallback={<p class={css.empty}>{t("mcpAudit.disabled", "The call audit is disabled.")}</p>}
			>
				<Show when={rows().length} fallback={<p class={css.empty}>{t("mcpAudit.empty", "No calls recorded yet.")}</p>}>
					<div class={css.tableWrap}>
						<table class={css.table}>
							<colgroup>
								<col class={css.colTime} />
								<col />
								<col class={css.colTarget} />
								<col class={css.colOutcome} />
								<col class={css.colBytes} />
								<col class={css.colEffect} />
							</colgroup>
							<thead>
								<tr>
									<th>{t("mcpAudit.col.time", "Time")}</th>
									<th>{t("mcpAudit.col.operation", "Operation")}</th>
									<th>{t("mcpAudit.col.target", "Target")}</th>
									<th>{t("mcpAudit.col.outcome", "Outcome")}</th>
									<th class={css.num}>{t("mcpAudit.col.bytes", "Bytes")}</th>
									<th>{t("mcpAudit.col.effect", "Effect")}</th>
								</tr>
							</thead>
							<tbody>
								<For each={rows()}>
									{(row) => {
										const outcome = () => OUTCOMES[row.outcome];
										const effect = () => EFFECTS[row.effect] ?? EFFECTS.unknown;
										const time = () => new Date(row.started_ms).toLocaleTimeString(locale(), { hour12: false });
										const outcomeLabel = () => outcome()?.label() ?? row.outcome;
										const targetLabel = () =>
											row.target
												? `${row.target.name ? row.target.name + " · " : ""}${row.target.id.slice(0, DIGEST_CHARS)}`
												: (row.target_digest?.slice(0, DIGEST_CHARS) ?? "—");
										const targetTitle = () =>
											row.target
												? `${row.target.name ? row.target.name + " · " : ""}${row.target.id}`
												: t("mcpAudit.targetTitle", "SHA-256 digest of the target; not the target");
										const bytes = () => String(row.response_text_bytes ?? "—");
										return (
											<tr>
												<td class={css.mono} title={time()}>
													{time()}
												</td>
												<td class={`${css.mono} ${css.clip}`} title={row.operation}>
													{row.operation}
												</td>
												<td class={`${css.mono} ${css.clip}`} title={targetTitle()}>
													{targetLabel()}
												</td>
												<td title={outcomeLabel()}>
													<StatusBadge label={outcomeLabel()} variant={outcome()?.variant} />
												</td>
												<td class={`${css.mono} ${css.num}`} title={bytes()}>
													{bytes()}
												</td>
												<td title={effect().hint()}>
													<StatusBadge label={effect().label()} variant={effect().variant} />
												</td>
											</tr>
										);
									}}
								</For>
							</tbody>
						</table>
					</div>
				</Show>
			</Show>
		</div>
	);
};
