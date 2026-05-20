import { type Component, createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { invoke } from "../../invoke";
import d from "../shared/dialog.module.css";
import s from "./GeneratorsModal.module.css";

type GeneratorId =
	| "password"
	| "uuid_v4"
	| "uuid_v7"
	| "ulid"
	| "cuid2"
	| "jwt_secret"
	| "totp_secret"
	| "nano_id"
	| "slug"
	| "ed25519_keypair";

interface GeneratorDef {
	id: GeneratorId;
	label: string;
	description: string;
	hasOptions: boolean;
}

const GENERATORS: GeneratorDef[] = [
	{ id: "password", label: "Password", description: "Random password", hasOptions: true },
	{ id: "uuid_v4", label: "UUID v4", description: "Standard random UUID", hasOptions: false },
	{ id: "uuid_v7", label: "UUID v7", description: "Time-ordered UUID", hasOptions: false },
	{ id: "ulid", label: "ULID", description: "Universally unique lexicographic ID", hasOptions: false },
	{ id: "cuid2", label: "CUID2", description: "Collision-resistant ID", hasOptions: false },
	{ id: "jwt_secret", label: "JWT Secret", description: "256-bit random hex key", hasOptions: false },
	{ id: "totp_secret", label: "TOTP Secret", description: "RFC 4226 base32 secret (160-bit)", hasOptions: false },
	{ id: "nano_id", label: "Nano ID", description: "URL-friendly random ID", hasOptions: true },
	{ id: "slug", label: "Slug", description: "adjective-noun-NNNN random slug", hasOptions: false },
	{
		id: "ed25519_keypair",
		label: "Ed25519 Key",
		description: "PKCS#8 PEM keypair (not OpenSSH format)",
		hasOptions: false,
	},
];

interface GeneratorResult {
	value: string;
	extra?: string;
}

export const GeneratorsModal: Component<{ onClose: () => void }> = (props) => {
	const [active, setActive] = createSignal<GeneratorId>("password");
	// SECURITY: generated values live only in these signals — cleared on unmount
	const [value, setValue] = createSignal("");
	const [extra, setExtra] = createSignal("");
	const [copied, setCopied] = createSignal(false);
	const [copiedExtra, setCopiedExtra] = createSignal(false);
	const [loading, setLoading] = createSignal(false);

	// Password options
	const [pwLen, setPwLen] = createSignal(32);
	const [pwUpper, setPwUpper] = createSignal(true);
	const [pwLower, setPwLower] = createSignal(true);
	const [pwNums, setPwNums] = createSignal(true);
	const [pwSymbols, setPwSymbols] = createSignal(true);

	// Nano ID options
	const [nanoLen, setNanoLen] = createSignal(21);

	onCleanup(() => {
		// SECURITY: wipe values from signal memory on close
		setValue("");
		setExtra("");
	});

	const buildRequest = () => {
		const id = active();
		if (id === "password")
			return {
				type: "password",
				length: pwLen(),
				uppercase: pwUpper(),
				lowercase: pwLower(),
				numbers: pwNums(),
				symbols: pwSymbols(),
			};
		if (id === "nano_id") return { type: "nano_id", length: nanoLen() };
		return { type: id };
	};

	const generate = async () => {
		setValue("");
		setExtra("");
		setCopied(false);
		setCopiedExtra(false);
		setLoading(true);
		try {
			// SECURITY: do not log result
			const result = await invoke<GeneratorResult>("generate_value", { request: buildRequest() });
			setValue(result.value);
			setExtra(result.extra ?? "");
		} finally {
			setLoading(false);
		}
	};

	const copy = async (text: string, setCopiedFn: (v: boolean) => void) => {
		if (!text) return;
		await navigator.clipboard.writeText(text);
		setCopiedFn(true);
		setTimeout(() => setCopiedFn(false), 2000);
	};

	// Escape to close
	const handleKey = (e: KeyboardEvent) => {
		if (e.key === "Escape") {
			e.preventDefault();
			e.stopPropagation();
			props.onClose();
		}
	};
	document.addEventListener("keydown", handleKey, true);
	onCleanup(() => document.removeEventListener("keydown", handleKey, true));

	// Auto-generate when active generator changes (or options change for password)
	createEffect(() => {
		active();
		void generate();
	});

	const activeDef = () => GENERATORS.find((g) => g.id === active())!;
	const isKeypair = () => active() === "ed25519_keypair";

	return (
		<div class={d.overlay} onClick={props.onClose}>
			<div class={`${d.popover} ${s.wide}`} onClick={(e) => e.stopPropagation()}>
				<div class={d.header}>
					<span class={d.headerIcon}>⚙</span>
					<h4>Generators</h4>
				</div>
				<div class={s.layout}>
					{/* Generator list */}
					<nav class={s.sidebar}>
						<For each={GENERATORS}>
							{(gen) => (
								<button class={`${s.tab} ${active() === gen.id ? s.active : ""}`} onClick={() => setActive(gen.id)}>
									{gen.label}
								</button>
							)}
						</For>
					</nav>

					{/* Output panel */}
					<div class={s.panel}>
						<p class={s.description}>{activeDef().description}</p>

						{/* Password options */}
						<Show when={active() === "password"}>
							<div class={s.options}>
								<div class={s.optionRow}>
									<label>Length: {pwLen()}</label>
									<input
										type="range"
										min={4}
										max={128}
										value={pwLen()}
										onInput={(e) => setPwLen(Number((e.target as HTMLInputElement).value))}
									/>
								</div>
								<div class={s.checkboxes}>
									<label class={s.checkboxLabel}>
										<input
											type="checkbox"
											checked={pwUpper()}
											onChange={(e) => setPwUpper((e.target as HTMLInputElement).checked)}
										/>{" "}
										A–Z
									</label>
									<label class={s.checkboxLabel}>
										<input
											type="checkbox"
											checked={pwLower()}
											onChange={(e) => setPwLower((e.target as HTMLInputElement).checked)}
										/>{" "}
										a–z
									</label>
									<label class={s.checkboxLabel}>
										<input
											type="checkbox"
											checked={pwNums()}
											onChange={(e) => setPwNums((e.target as HTMLInputElement).checked)}
										/>{" "}
										0–9
									</label>
									<label class={s.checkboxLabel}>
										<input
											type="checkbox"
											checked={pwSymbols()}
											onChange={(e) => setPwSymbols((e.target as HTMLInputElement).checked)}
										/>{" "}
										!@#…
									</label>
								</div>
							</div>
						</Show>

						{/* Nano ID length option */}
						<Show when={active() === "nano_id"}>
							<div class={s.options}>
								<div class={s.optionRow}>
									<label>Length</label>
									<input
										type="number"
										min={4}
										max={64}
										value={nanoLen()}
										onInput={(e) => setNanoLen(Number((e.target as HTMLInputElement).value))}
									/>
								</div>
							</div>
						</Show>

						{/* Single value output */}
						<Show when={!isKeypair()}>
							<div class={s.outputGroup}>
								<textarea class={s.output} readOnly value={loading() ? "Generating…" : value()} />
							</div>
						</Show>

						{/* Keypair output */}
						<Show when={isKeypair()}>
							<div class={s.outputGroup}>
								<span class={s.outputGroupLabel}>Private key (PKCS#8)</span>
								<textarea class={s.output} readOnly value={loading() ? "Generating…" : value()} />
							</div>
							<div class={s.outputGroup}>
								<span class={s.outputGroupLabel}>Public key (SPKI)</span>
								<textarea class={s.output} readOnly value={loading() ? "Generating…" : extra()} />
							</div>
							<p class={s.note}>Not OpenSSH format — for SSH keys use ssh-keygen -t ed25519</p>
						</Show>
					</div>
				</div>

				<div class={s.actions}>
					<Show
						when={!isKeypair()}
						fallback={
							<>
								<button
									class={s.copyBtn}
									disabled={!value() || loading()}
									onClick={() => void copy(value(), setCopied)}
								>
									{copied() ? "Copied!" : "Copy Private"}
								</button>
								<button
									class={s.copyBtn}
									disabled={!extra() || loading()}
									onClick={() => void copy(extra(), setCopiedExtra)}
								>
									{copiedExtra() ? "Copied!" : "Copy Public"}
								</button>
							</>
						}
					>
						<button class={s.copyBtn} disabled={!value() || loading()} onClick={() => void copy(value(), setCopied)}>
							{copied() ? "Copied!" : "Copy"}
						</button>
					</Show>
					<button class={s.secondaryBtn} disabled={loading()} onClick={() => void generate()}>
						Regenerate
					</button>
					<div class={s.spacer} />
					<button class={s.secondaryBtn} onClick={props.onClose}>
						Close
					</button>
				</div>
			</div>
		</div>
	);
};
