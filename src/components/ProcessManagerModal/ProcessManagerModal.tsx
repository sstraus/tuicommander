import { type Component, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "../../invoke";
import { appLogger } from "../../stores/appLogger";
import d from "../shared/dialog.module.css";
import s from "./ProcessManagerModal.module.css";

interface ProcessStats {
	session_id: string | null;
	name: string;
	pid: number;
	rss_kb: number;
	cpu_pct: number;
}

type SortKey = "name" | "pid" | "cpu" | "mem";

function formatMemory(kb: number): string {
	if (kb < 1024) return `${kb} KB`;
	const mb = kb / 1024;
	if (mb < 1024) return `${mb.toFixed(1)} MB`;
	return `${(mb / 1024).toFixed(2)} GB`;
}

interface ProcessManagerModalProps {
	onClose: () => void;
}

export const ProcessManagerModal: Component<ProcessManagerModalProps> = (props) => {
	const [processes, setProcesses] = createSignal<ProcessStats[]>([]);
	const [sortKey, setSortKey] = createSignal<SortKey>("mem");
	const [sortAsc, setSortAsc] = createSignal(false);
	let timer: ReturnType<typeof setInterval> | undefined;

	async function refresh(): Promise<void> {
		try {
			const stats = await invoke<ProcessStats[]>("get_process_stats");
			setProcesses(stats);
		} catch (err) {
			appLogger.error("app", "Failed to get process stats", err);
		}
	}

	onMount(() => {
		void refresh();
		timer = setInterval(() => void refresh(), 2000);
	});

	onCleanup(() => {
		if (timer) clearInterval(timer);
	});

	function handleSort(key: SortKey): void {
		if (sortKey() === key) {
			setSortAsc(!sortAsc());
		} else {
			setSortKey(key);
			setSortAsc(false);
		}
	}

	function sortIndicator(key: SortKey): string {
		if (sortKey() !== key) return "";
		return sortAsc() ? " ▲" : " ▼";
	}

	function sorted(): ProcessStats[] {
		const list = [...processes()];
		const key = sortKey();
		const asc = sortAsc();
		list.sort((a, b) => {
			let cmp = 0;
			switch (key) {
				case "name":
					cmp = a.name.localeCompare(b.name);
					break;
				case "pid":
					cmp = a.pid - b.pid;
					break;
				case "cpu":
					cmp = a.cpu_pct - b.cpu_pct;
					break;
				case "mem":
					cmp = a.rss_kb - b.rss_kb;
					break;
			}
			return asc ? cmp : -cmp;
		});
		return list;
	}

	function totalMemory(): number {
		return processes().reduce((sum, p) => sum + p.rss_kb, 0);
	}

	function maxMem(): number {
		return Math.max(1, ...processes().map((p) => p.rss_kb));
	}

	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div class={d.popover} style={{ width: "620px", "max-width": "90vw" }} onClick={(e) => e.stopPropagation()}>
				<div class={d.header}>
					<div class={d.headerText}>
						<h4>Process Manager</h4>
					</div>
				</div>
				<div class={s.scrollBody}>
					<Show when={processes().length > 0} fallback={<div class={s.empty}>Loading...</div>}>
						<table class={s.table}>
							<thead>
								<tr>
									<th class={s.sortable} onClick={() => handleSort("name")}>
										Process{sortIndicator("name")}
									</th>
									<th class={`${s.sortable} ${s.right}`} onClick={() => handleSort("pid")}>
										PID{sortIndicator("pid")}
									</th>
									<th class={`${s.sortable} ${s.right}`} onClick={() => handleSort("cpu")}>
										CPU{sortIndicator("cpu")}
									</th>
									<th class={`${s.sortable} ${s.right}`} onClick={() => handleSort("mem")}>
										Memory{sortIndicator("mem")}
									</th>
									<th class={s.barCell} />
								</tr>
							</thead>
							<tbody>
								<For each={sorted()}>
									{(proc) => {
										const isTuic = () => proc.session_id === null;
										const memPct = () => (proc.rss_kb / maxMem()) * 100;
										const cpuHigh = () => proc.cpu_pct > 50;
										return (
											<tr
												classList={{
													[s.tuicRow]: isTuic(),
													[s.childRow]: !isTuic() && proc.name !== proc.session_id,
												}}
											>
												<td title={proc.name}>{proc.name}</td>
												<td class={`${s.pid} ${s.right}`}>{proc.pid}</td>
												<td class={s.right}>{proc.cpu_pct.toFixed(1)}%</td>
												<td class={s.right}>{formatMemory(proc.rss_kb)}</td>
												<td class={s.barCell}>
													<div class={s.bar}>
														<div
															class={`${s.barFill} ${cpuHigh() ? s.highCpu : s.memFill}`}
															style={{
																transform: `scaleX(${memPct() / 100})`,
															}}
														/>
													</div>
												</td>
											</tr>
										);
									}}
								</For>
							</tbody>
						</table>
					</Show>
				</div>
				<div class={s.footer}>
					<span>
						<span class={s.refreshIndicator} />
						Auto-refresh 2s
					</span>
					<span>
						Total: {formatMemory(totalMemory())} across {processes().length} processes
					</span>
				</div>
			</div>
		</div>
	);
};
