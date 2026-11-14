import { createSignal } from "solid-js";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
export function ActivityPresets(props: {
	busy: boolean;
	onSave: (id: string, revision: number) => Promise<void>;
	onLoad: (id: string, revision: number) => Promise<void>;
}) {
	const [id, setId] = createSignal("");
	const [revision, setRevision] = createSignal(1);
	return (
		<section aria-label="Activity presets">
			<label>
				Preset ID
				<Input value={id()} onInput={(event) => setId(event.currentTarget.value)} />
			</label>
			<label>
				Preset revision
				<Input
					type="number"
					min="1"
					value={revision()}
					onInput={(event) => setRevision(Number(event.currentTarget.value))}
				/>
			</label>
			<Button disabled={props.busy || !id()} onClick={() => void props.onSave(id(), revision())}>
				Save configuration preset
			</Button>
			<Button disabled={props.busy || !id()} onClick={() => void props.onLoad(id(), revision())}>
				Load configuration preset
			</Button>
		</section>
	);
}
