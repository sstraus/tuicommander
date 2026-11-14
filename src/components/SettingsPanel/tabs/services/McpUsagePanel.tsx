import { type Component, createSignal, For, onMount, Show } from "solid-js";
import { t } from "../../../../i18n";
import { appLogger } from "../../../../stores/appLogger";
import { rpc } from "../../../../transport";
import { Button } from "../../../ui/Button";
import s from "../../Settings.module.css";
import css from "./McpAuditPanel.module.css";

/** Body-free rows of `GET /mcp/usage`: the last 7 days per role. */
interface UsageAction {
	tool: string;
	action: string;
	calls: number;
	application_errors: number;
	invalid_arguments: number;
	response_bytes_p95: number;
}

interface UsageRole {
	role: string;
	calls: number;
	application_errors: number;
	invalid_arguments: number;
	catalogue_bytes: number;
	actions: UsageAction[];
}

interface UsageReport {
	enabled: boolean;
	days: number;
	roles: UsageRole[];
}

const TOP_ACTIONS = 8;

/** MCP usage rollup: last 7 days per role; fetched on open and on demand only. */
export const McpUsagePanel: Component = () => {
	const [report, setReport] = createSignal<UsageReport | null>(null);
	const [failed, setFailed] = createSignal(false);

	const refresh = async () => {
		try {
			setReport(await rpc<UsageReport>("get_mcp_usage"));
			setFailed(false);
		} catch (error) {
			appLogger.warn("config", "Failed to load MCP usage", error);
			setFailed(true);
		}
	};
	onMount(refresh);

	return (
		<div>
			<div class={css.toolbar}>
				<span class={css.spacer} />
				<Button
					type="button"
					class={s.testBtn}
					data-non-setting="Reload the usage rows; not a preference"
					onClick={refresh}
				>
					{t("mcpUsage.refresh", "Refresh")}
				</Button>
			</div>
			<Show when={failed()}>
				<p class={css.empty} role="alert">
					{t("mcpUsage.failed", "Could not load the usage counts.")}
				</p>
			</Show>
			<Show
				when={report()?.roles.length}
				fallback={<p class={css.empty}>{t("mcpUsage.empty", "No calls counted in the last 7 days.")}</p>}
			>
				<For each={report()?.roles}>
					{(role) => (
						<div class={css.tableWrap}>
							<p>
								<strong>{role.role}</strong>{" "}
								{t("mcpUsage.summary", "{calls} calls · {errors} errors · {invalid} rejected · catalogue {bytes} B", {
									calls: String(role.calls),
									errors: String(role.application_errors),
									invalid: String(role.invalid_arguments),
									bytes: String(role.catalogue_bytes),
								})}
							</p>
							<table class={css.table}>
								<thead>
									<tr>
										<th>{t("mcpUsage.col.operation", "Operation")}</th>
										<th class={css.num}>{t("mcpUsage.col.calls", "Calls")}</th>
										<th class={css.num}>{t("mcpUsage.col.errors", "Errors")}</th>
										<th class={css.num}>{t("mcpUsage.col.invalid", "Rejected")}</th>
										<th class={css.num}>{t("mcpUsage.col.p95", "p95 bytes")}</th>
									</tr>
								</thead>
								<tbody>
									<For each={role.actions.slice(0, TOP_ACTIONS)}>
										{(a) => (
											<tr>
												<td class={`${css.mono} ${css.clip}`}>{a.action ? `${a.tool}.${a.action}` : a.tool}</td>
												<td class={`${css.mono} ${css.num}`}>{a.calls}</td>
												<td class={`${css.mono} ${css.num}`}>{a.application_errors}</td>
												<td class={`${css.mono} ${css.num}`}>{a.invalid_arguments}</td>
												<td class={`${css.mono} ${css.num}`}>{a.response_bytes_p95}</td>
											</tr>
										)}
									</For>
								</tbody>
							</table>
						</div>
					)}
				</For>
			</Show>
		</div>
	);
};
