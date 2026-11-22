import { For } from "solid-js";
import { Button } from "../ui/Button";
import type { ActivityDescriptor } from "./catalogTypes";
import type { NeutralPublication } from "./neutralClient";
import type { useNeutralDraft } from "./useNeutralDraft";
import s from "./WorkflowDesigner.module.css";

/** Operation ids allow ASCII letters, digits, '.', '_' and '-' (catalog.rs valid_name). */
function operationName(label: string): string {
	const slug = label
		.toLowerCase()
		.replace(/[^a-z0-9._-]+/g, "-")
		.replace(/^-+|-+$/g, "");
	return slug || "subflow";
}

/** The catalog operation that already runs exactly this publication revision, if any. */
function existingOperation(activities: ActivityDescriptor[], item: NeutralPublication) {
	return activities.find((descriptor) => {
		if (descriptor.target?.type !== "subflow") return false;
		const pin = descriptor.target.publication as { reference?: { project?: string; id?: string; revision?: number } };
		return (
			pin.reference?.project === item.project &&
			pin.reference.id === item.id &&
			pin.reference.revision === item.revision
		);
	});
}

/**
 * Published workflows of this project. "Add as subflow" reuses the operation already pinned to
 * that revision, or asks the backend to register one; the backend computes the pin and interface.
 */
export function SubflowPalette(props: {
	state: ReturnType<typeof useNeutralDraft>;
	onInspect: (item: NeutralPublication) => void;
	onAdd: (descriptor: ActivityDescriptor) => void;
}) {
	const label = (item: NeutralPublication) =>
		props.state.drafts().find((draft) => draft.id === item.id)?.name ?? item.id;
	async function addAsSubflow(item: NeutralPublication) {
		const activities = props.state.catalog().activities;
		const reused = existingOperation(activities, item);
		if (reused) return props.onAdd(reused);
		const name = operationName(`${label(item)}-r${item.revision}`);
		const current = Math.max(
			0,
			...activities
				.filter((descriptor) => descriptor.id.namespace === "project" && descriptor.id.name === name)
				.map((descriptor) => descriptor.id.version),
		);
		const descriptor = await props.state.call(
			{
				action: "save_subflow",
				expected_version: current,
				id: { namespace: "project", name, version: current + 1 },
				publication: { project: item.project, id: item.id, revision: item.revision },
			},
			"operation",
		);
		await props.state.refresh();
		props.onAdd(descriptor);
	}
	return (
		<div class={s.palette}>
			<For each={props.state.catalog().metanodes}>
				{(item) => (
					<>
						<Button title={`Inspect ${item.id} · revision ${item.revision}`} onClick={() => props.onInspect(item)}>
							<span class={s.catalogName}>
								Inspect {label(item)} · revision {item.revision}
							</span>
						</Button>
						<Button
							title={`Add ${label(item)} revision ${item.revision} as a subflow activity`}
							onClick={() => void props.state.perform(() => addAsSubflow(item))}
						>
							<span class={s.catalogName}>
								Add as subflow: {label(item)} · revision {item.revision}
							</span>
						</Button>
					</>
				)}
			</For>
		</div>
	);
}
