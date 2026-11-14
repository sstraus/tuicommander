/** Native descriptor used by dialog mocks for the shared backend_status read. */
export const nativeBackendStatusReply = {
	type: "backend_status",
	value: {
		backend: "native",
		label: "TUIC native",
		provenance: "automatic",
		availability: "available",
		reason: "No Wiz project markers",
		installation: null,
		version: null,
		error: null,
	},
};
