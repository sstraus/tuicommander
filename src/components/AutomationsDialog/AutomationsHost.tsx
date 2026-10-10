import { lazy, Show, Suspense } from "solid-js";
import { automationsUi } from "../../stores/automations";

const Dialog = lazy(() => import("./AutomationsDialog").then((module) => ({ default: module.AutomationsDialog })));
export function AutomationsHost() {
	return (
		<Show when={automationsUi.visible()}>
			<Suspense>
				<Dialog onClose={automationsUi.close} />
			</Suspense>
		</Show>
	);
}
