import { createSignal, For, Show } from "solid-js";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import s from "./CorrectionRegion.module.css";
import type { JsonValue } from "./catalogTypes";
import {
	type CorrectionRegionView,
	type NeutralPublication,
	neutralClient,
	type CorrectionRegion as Region,
} from "./neutralClient";
import type { useNeutralDraft } from "./useNeutralDraft";

/** Backend lowering owns the generated graph and its correction budget. */
export function CorrectionRegionEditor(props: { state: ReturnType<typeof useNeutralDraft> }) {
	const [source, setSource] = createSignal("");
	const [view, setView] = createSignal<CorrectionRegionView>();
	const [child, setChild] = createSignal<NeutralPublication>();
	return (
		<details class={s.panel}>
			<summary>Bounded correction region</summary>
			<label>
				Correction region (JSON)
				<Textarea
					value={source()}
					disabled={props.state.busy()}
					onInput={(event) => {
						setSource(event.currentTarget.value);
						setView(undefined);
						setChild(undefined);
					}}
				/>
			</label>
			<Button
				disabled={props.state.busy() || !source().trim()}
				onClick={() =>
					void props.state.perform(async () => {
						const region = JSON.parse(source()) as Region;
						setView(await props.state.call({ action: "correction_region", region }, "correction_region"));
					})
				}
			>
				Inspect correction region
			</Button>
			<Show when={view()}>
				{(result) => (
					<section aria-label="Correction region contract">
						<h4>Correction criteria</h4>
						<For each={result().region.criteria.criteria}>{(criterion) => <p>{criterion}</p>}</For>
						<p>Maximum corrections: {result().region.max_corrections}</p>
						<h4>Selected activity</h4>
						<pre>{JSON.stringify(result().region.selected, null, 2)}</pre>
						<h4>Generated inputs, outputs, control, loops and subflows</h4>
						<pre>{JSON.stringify(result().definition, null, 2)}</pre>
						<For each={result().definition.subflows}>
							{(reference) => (
								<Button
									disabled={props.state.busy()}
									onClick={() =>
										void props.state.perform(async () => {
											const call = neutralClient(() => reference.project);
											setChild(
												await call(
													{ action: "get_publication", id: reference.id, revision: reference.revision },
													"publication",
												),
											);
										})
									}
								>
									Inspect selected child {reference.id} · revision {reference.revision}
								</Button>
							)}
						</For>
						<Button
							disabled={props.state.busy()}
							onClick={() =>
								void props.state.perform(async () => {
									const definition = result().definition;
									const checked = await props.state.call({ action: "validate", definition }, "validation");
									if (!checked.valid) {
										props.state.setError(checked.diagnostics.map((item) => `${item.path}: ${item.message}`).join("\n"));
										return;
									}
									props.state.edit(definition);
								})
							}
						>
							Use generated correction definition
						</Button>
					</section>
				)}
			</Show>
			<Show when={child()}>
				{(publication) => (
					<section aria-label="Selected correction child publication">
						<h4>
							{publication().id} · revision {publication().revision} · read only
						</h4>
						<pre>{JSON.stringify(publication(), null, 2)}</pre>
					</section>
				)}
			</Show>
		</details>
	);
}

function record(value: JsonValue | undefined): { [key: string]: JsonValue } | undefined {
	return value !== null && typeof value === "object" && !Array.isArray(value) ? value : undefined;
}

/** Presentation of retained scope data, never a separate correction counter. */
export function RetainedCorrections(props: { scope: JsonValue | undefined }) {
	const identity = () => record(record(props.scope)?.identity);
	const kind = () => record(record(props.scope)?.kind);
	const iteration = () => kind()?.iteration;
	return (
		<Show
			when={
				identity()?.scope === "correction" &&
				kind()?.type === "loop" &&
				typeof iteration() === "number" &&
				Number.isInteger(iteration()) &&
				Number(iteration()) >= 1
			}
		>
			<section aria-label="Retained correction budget">
				<p>Consumed corrections: {Number(iteration()) - 1}</p>
				<p>Correction scope generation: {String(identity()?.generation)}</p>
			</section>
		</Show>
	);
}
