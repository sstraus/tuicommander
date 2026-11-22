import { createContext, useContext } from "solid-js";
import type { ActivityDescriptor } from "./catalogTypes";
import type { NeutralDefinition } from "./neutralClient";
import { activityTitle } from "./neutralGraph";

/** What the open draft offers as binding sources; absent outside the Designer. */
export interface BindingChoices {
	definition: () => NeutralDefinition;
	activities: () => ActivityDescriptor[];
	labels: () => Record<string, string>;
}
export const BindingChoicesContext = createContext<BindingChoices>();
export const useBindingChoices = () => useContext(BindingChoicesContext);

export function activityOptions(choices: BindingChoices) {
	const definition = choices.definition();
	return definition.instances.map((_, ordinal) => ({
		ordinal,
		title: activityTitle(definition, choices.labels(), ordinal),
	}));
}

/** Output field names of one activity, or undefined when its descriptor is not in the catalog. */
export function activityOutputFields(choices: BindingChoices, ordinal: number): string[] | undefined {
	const id = choices.definition().instances[ordinal]?.descriptor;
	const descriptor = choices
		.activities()
		.find(
			(item) => item.id.namespace === id?.namespace && item.id.name === id?.name && item.id.version === id?.version,
		);
	return descriptor ? Object.keys(descriptor.outputs.fields) : undefined;
}
