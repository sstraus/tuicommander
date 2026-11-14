import { For, onCleanup, Show } from "solid-js";
import { waitingItemsStore } from "../../stores/waitingItems";
import { Button } from "../ui/Button";
import s from "./AgentHealthView.module.css";

/** Agent failures and stalls. Items that wait on the user live in Plans and Stories and the bell. */
export function AgentHealthView(props: { onOpenSession: (sessionId: string, connectionId: string | null) => void }) {
	onCleanup(waitingItemsStore.retain({ health: true }));
	return (
		<section class={s.health} aria-label="Agent health" aria-busy={waitingItemsStore.loading()}>
			<div class={s.heading}>
				<h4>Agent health</h4>
				<Button size="compact" onClick={() => void waitingItemsStore.refresh()}>
					Refresh health
				</Button>
			</div>
			<Show when={waitingItemsStore.failed()}>
				<p role="alert">Agent health is unavailable. Refresh to try again.</p>
			</Show>
			<Show when={waitingItemsStore.liveFailed()}>
				<p role="alert">Live updates are unavailable. Use Refresh health.</p>
			</Show>
			<Show when={waitingItemsStore.snapshot()}>
				{(data) => (
					<>
						<p class={s.muted}>
							Current session facts and registered local projects. Stall flags come from the owning host.
						</p>
						<Show when={data().health.length === 0}>
							<p>No agent failures or stalls reported.</p>
						</Show>
						<For each={data().health}>
							{(agent) => (
								<article class={s.item}>
									<strong>
										{agent.kind} · {agent.name}
									</strong>
									<p>{agent.reason}</p>
									<Show when={agent.connectionId}>
										<p>Remote: {agent.connectionId}</p>
									</Show>
									<Button size="compact" onClick={() => props.onOpenSession(agent.sessionId, agent.connectionId)}>
										Inspect terminal
									</Button>
								</article>
							)}
						</For>
					</>
				)}
			</Show>
		</section>
	);
}
