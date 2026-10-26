import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { Button } from "../../components/ui/Button";
import { ButtonGroup } from "../../components/ui/ButtonGroup";
import { ColorInput } from "../../components/ui/ColorInput";
import { ColorSwatch } from "../../components/ui/ColorSwatch";
import { Input } from "../../components/ui/Input";
import { SegmentedControl } from "../../components/ui/SegmentedControl";
import { Select } from "../../components/ui/Select";
import { Slider } from "../../components/ui/Slider";
import { Switch } from "../../components/ui/Switch";
import { Textarea } from "../../components/ui/Textarea";

describe("shared form controls", () => {
	// Catches: compact being dropped (a 36px field beside 20px buttons) or leaking onto the DOM as an attribute.
	it("adds the compact size class without leaking the prop", () => {
		render(() => (
			<>
				<Select aria-label="Plain">
					<option>a</option>
				</Select>
				<Select compact aria-label="Dense">
					<option>a</option>
				</Select>
				<Input compact aria-label="Dense input" />
			</>
		));
		const plain = screen.getByRole("combobox", { name: "Plain" });
		const dense = screen.getByRole("combobox", { name: "Dense" });
		expect(dense.className.split(" ").length).toBe(plain.className.split(" ").length + 1);
		expect(dense).not.toHaveAttribute("compact");
		expect(screen.getByRole("textbox", { name: "Dense input" })).not.toHaveAttribute("compact");
	});

	// Catches: Input stripping `compact` from the DOM props but never adding the size class, so Error Log search,
	// Branches create/rename and Agents env-flag fields stay 36px beside 22px neighbours.
	it("adds the compact size class to Input and keeps a caller class", () => {
		render(() => (
			<>
				<Input aria-label="Plain" class="mine" />
				<Input compact aria-label="Dense" class="mine" />
			</>
		));
		const plain = screen.getByRole("textbox", { name: "Plain" });
		const dense = screen.getByRole("textbox", { name: "Dense" });
		expect(dense.classList.length).toBe(plain.classList.length + 1);
		expect(dense).toHaveClass("mine");
	});

	// Catches: a wrapper freezing reactive props or losing native ref/event targets and form values.
	it("preserves reactive selection, native refs, names and change targets", () => {
		const [value, setValue] = createSignal("board");
		const [disabled, setDisabled] = createSignal(false);
		const [invalid, setInvalid] = createSignal(true);
		const changed = vi.fn();
		let element: HTMLSelectElement | undefined;
		render(() => (
			<form aria-label="Views">
				<Select
					aria-label="View"
					name="view"
					ref={(node) => {
						element = node;
					}}
					value={value()}
					disabled={disabled()}
					class="caller-layout"
					classList={{ invalid: invalid() }}
					onChange={(event) => changed(event.currentTarget.value)}
				>
					<option value="list">List</option>
					<option value="board">Board</option>
				</Select>
			</form>
		));
		const select = screen.getByRole("combobox", { name: "View" });
		expect(element).toBe(select);
		expect(select).toHaveValue("board");
		setValue("list");
		expect(select).toHaveValue("list");
		fireEvent.change(select, { target: { value: "board" } });
		expect(changed).toHaveBeenCalledWith("board");
		expect(new FormData(screen.getByRole("form", { name: "Views" }) as HTMLFormElement).get("view")).toBe("board");
		expect(select).toHaveClass("caller-layout", "invalid");
		setInvalid(false);
		expect(select).not.toHaveClass("invalid");
		setDisabled(true);
		expect(select).toBeDisabled();
	});

	// Catches: flattening selected children into a single value, breaking multi-select task fields.
	it("preserves multiple options selected through child attributes", () => {
		render(() => (
			<Select multiple aria-label="Labels">
				<option value="a" selected>
					A
				</option>
				<option value="b">B</option>
				<option value="c" selected>
					C
				</option>
			</Select>
		));
		const select = screen.getByRole("listbox", { name: "Labels" }) as HTMLSelectElement;
		expect(Array.from(select.selectedOptions, (option) => option.value)).toEqual(["a", "c"]);
	});

	// Catches: dropping input events, constraints, or reactive values while forwarding text controls.
	it("keeps input and textarea edits and reactive values at the native boundary", () => {
		const [text, setText] = createSignal("draft");
		render(() => (
			<>
				<Input
					aria-label="Title"
					name="title"
					required
					value={text()}
					onInput={(event) => setText(event.currentTarget.value)}
				/>
				<Textarea aria-label="Description" value={text()} readOnly />
			</>
		));
		const input = screen.getByRole("textbox", { name: "Title" });
		fireEvent.input(input, { target: { value: "edited" } });
		expect(input).toBeRequired();
		expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue("edited");
		expect(screen.getByRole("textbox", { name: "Description" })).toHaveAttribute("readonly");
	});

	// Catches: keyboard navigation choosing disabled segments or leaving focus and selection out of sync.
	it("switches views with clicks and arrow/Home/End keys, skipping disabled choices", () => {
		const [view, setView] = createSignal("list");
		render(() => (
			<SegmentedControl
				aria-label="Tasks view"
				value={view()}
				onChange={setView}
				options={[
					{ id: "list", label: "List" },
					{ id: "board", label: "Board", disabled: true },
					{ id: "roadmap", label: "Roadmap" },
				]}
			/>
		));
		const list = screen.getByRole("tab", { name: "List" });
		const roadmap = screen.getByRole("tab", { name: "Roadmap" });
		fireEvent.keyDown(list, { key: "ArrowRight" });
		expect(roadmap).toHaveAttribute("aria-selected", "true");
		expect(roadmap).toHaveFocus();
		fireEvent.keyDown(roadmap, { key: "ArrowRight" });
		expect(list).toHaveFocus();
		fireEvent.keyDown(list, { key: "End" });
		expect(roadmap).toHaveFocus();
		fireEvent.keyDown(roadmap, { key: "Home" });
		expect(list).toHaveAttribute("tabindex", "0");
		fireEvent.click(roadmap);
		expect(roadmap).toHaveAttribute("aria-selected", "true");
	});
});

it("catches Button losing native submit, reactive disabled state or caller events", () => {
	const [disabled, setDisabled] = createSignal(false);
	const clicked = vi.fn();
	let native: HTMLButtonElement | undefined;
	render(() => (
		<>
			<Button
				ref={(node) => {
					native = node;
				}}
				class="caller"
				variant="danger"
				size="compact"
				disabled={disabled()}
				onClick={clicked}
			>
				Delete
			</Button>
			<Button type="submit">Save</Button>
		</>
	));
	const action = screen.getByRole("button", { name: "Delete" });
	expect(native).toBe(action);
	expect(action).toHaveAttribute("type", "button");
	expect(action).toHaveClass("caller", "compact", "danger");
	expect(action).not.toHaveAttribute("variant");
	expect(action).not.toHaveAttribute("size");
	fireEvent.click(action);
	expect(clicked).toHaveBeenCalledOnce();
	setDisabled(true);
	expect(action).toBeDisabled();
	expect(screen.getByRole("button", { name: "Save" })).toHaveAttribute("type", "submit");
});

// Catches: promotion freezes native state or redirects callbacks/refs onto a decorative wrapper.
it("keeps specialized controls reactive and forwards native refs and input/change boundaries", () => {
	const [checked, setChecked] = createSignal(false);
	const [value, setValue] = createSignal(5);
	const [color, setColor] = createSignal("#123456");
	const [disabled, setDisabled] = createSignal(false);
	const commit = vi.fn();
	let toggle: HTMLInputElement | undefined;
	let picker: HTMLInputElement | undefined;
	render(() => (
		<>
			<Switch
				aria-label="Enabled"
				ref={(node) => {
					toggle = node;
				}}
				checked={checked()}
				disabled={disabled()}
				onChange={(event) => setChecked(event.currentTarget.checked)}
			/>
			<Slider
				aria-label="Volume"
				min={0}
				max={10}
				step={1}
				value={value()}
				disabled={disabled()}
				onInput={(event) => setValue(event.currentTarget.valueAsNumber)}
				onChange={(event) => commit(event.currentTarget.valueAsNumber)}
			/>
			<ColorInput
				aria-label="Accent"
				ref={(node) => {
					picker = node;
				}}
				value={color()}
				disabled={disabled()}
				onInput={(event) => setColor(event.currentTarget.value)}
			/>
		</>
	));
	const input = screen.getByRole("switch", { name: "Enabled" });
	expect(toggle).toBe(input);
	fireEvent.change(input, { target: { checked: true } });
	expect(checked()).toBe(true);
	setChecked(false);
	expect(input).not.toBeChecked();
	const range = screen.getByRole("slider", { name: "Volume" });
	fireEvent.input(range, { target: { value: "8" } });
	expect(value()).toBe(8);
	expect(commit).not.toHaveBeenCalled();
	fireEvent.change(range, { target: { value: "8" } });
	expect(commit).toHaveBeenCalledExactlyOnceWith(8);
	setValue(3);
	expect(range).toHaveValue("3");
	const nativeColor = screen.getByLabelText("Accent");
	expect(picker).toBe(nativeColor);
	fireEvent.input(nativeColor, { target: { value: "#abcdef" } });
	expect(color()).toBe("#abcdef");
	setColor("#654321");
	expect(nativeColor).toHaveValue("#654321");
	setDisabled(true);
	for (const control of [input, range, nativeColor]) expect(control).toBeDisabled();
});

// Catches: swatch promotion submits its enclosing form or freezes native owner state/ref/events.
it("keeps preset and clear swatches native, reactive and non-submitting", () => {
	const [selected, setSelected] = createSignal(false);
	const [disabled, setDisabled] = createSignal(false);
	let native: HTMLButtonElement | undefined;
	const clicked = vi.fn();
	render(() => (
		<ColorSwatch
			aria-label="Blue"
			swatchColor="#123456"
			selected={selected()}
			disabled={disabled()}
			ref={(node) => {
				native = node;
			}}
			onClick={(event) => {
				clicked(event.currentTarget);
				setSelected(true);
			}}
		/>
	));
	const button = screen.getByRole("button", { name: "Blue" });
	expect(native).toBe(button);
	expect(button).toHaveAttribute("type", "button");
	expect(button).toHaveAttribute("aria-pressed", "false");
	fireEvent.click(button);
	expect(clicked).toHaveBeenCalledExactlyOnceWith(button);
	expect(button).toHaveAttribute("aria-pressed", "true");
	setSelected(false);
	expect(button).toHaveAttribute("aria-pressed", "false");
	setDisabled(true);
	expect(button).toBeDisabled();
});

// Catches: stretch geometry leaking onto the native element or swallowing caller focus/props.
it("keeps stretch actions native and consumes the geometry prop", () => {
	render(() => (
		<Button geometry="stretch" aria-label="Edge action">
			Edge
		</Button>
	));
	const button = screen.getByRole("button", { name: "Edge action" });
	expect(button).not.toHaveAttribute("geometry");
	button.focus();
	expect(button).toHaveFocus();
	expect(button).toHaveAttribute("type", "button");
});

// Catches: grouping actions replacing native buttons or merging independent disabled/click state.
it("keeps joined actions independently focusable and reactive", () => {
	const [disabled, setDisabled] = createSignal(false);
	const main = vi.fn(),
		menu = vi.fn();
	render(() => (
		<ButtonGroup aria-label="Prompt actions">
			<Button onClick={main} disabled={disabled()}>
				Run
			</Button>
			<Button onClick={menu} aria-expanded="false">
				Choose
			</Button>
		</ButtonGroup>
	));
	const group = screen.getByRole("group", { name: "Prompt actions" });
	const run = screen.getByRole("button", { name: "Run" });
	const choose = screen.getByRole("button", { name: "Choose" });
	expect(group).toContainElement(run);
	fireEvent.click(run);
	fireEvent.click(choose);
	expect(main).toHaveBeenCalledOnce();
	expect(menu).toHaveBeenCalledOnce();
	setDisabled(true);
	expect(run).toBeDisabled();
	expect(choose).not.toBeDisabled();
	choose.focus();
	expect(choose).toHaveFocus();
	expect(choose).toHaveAttribute("aria-expanded", "false");
});

// Catches: auto-grow swallowing native refs, bound input handlers or controlled value updates.
it("keeps auto-grow textarea edits and owner refs at the native boundary", () => {
	const [value, setValue] = createSignal("draft");
	let native: HTMLTextAreaElement | undefined;
	const input = vi.fn();
	render(() => (
		<Textarea
			autoGrow
			aria-label="Commit"
			ref={(node) => {
				native = node;
			}}
			value={value()}
			onInput={[
				(_data, event) => {
					input(event.currentTarget.value);
					setValue(event.currentTarget.value);
				},
				"owner",
			]}
		/>
	));
	const field = screen.getByRole("textbox", { name: "Commit" });
	expect(native).toBe(field);
	expect(field).not.toHaveAttribute("autoGrow");
	fireEvent.input(field, { target: { value: "edited" } });
	expect(input).toHaveBeenCalledExactlyOnceWith("edited");
	setValue("replaced");
	expect(field).toHaveValue("replaced");
	field.focus();
	expect(field).toHaveFocus();
});

// Catches: state promotion freezing native validity/descriptions or losing toggle callbacks.
it("forwards reactive invalid fields and removable toggle state", () => {
	const [invalid, setInvalid] = createSignal(true),
		[pressed, setPressed] = createSignal(false);
	render(() => (
		<>
			<Input aria-label="Invalid input" aria-invalid={invalid()} aria-describedby="error" />
			<Select aria-label="Invalid select" aria-invalid={invalid()} aria-describedby="error">
				<option>One</option>
			</Select>
			<Textarea aria-label="Invalid textarea" aria-invalid={invalid()} aria-describedby="error" />
			<p id="error">Existing validation error</p>
			<Button variant="removalToggle" aria-pressed={pressed()} onClick={() => setPressed(!pressed())}>
				Integration
			</Button>
		</>
	));
	for (const name of ["Invalid input", "Invalid select", "Invalid textarea"]) {
		const control = screen.getByLabelText(name);
		expect(control).toHaveAttribute("aria-invalid", "true");
		expect(control).toHaveAccessibleDescription("Existing validation error");
	}
	setInvalid(false);
	for (const name of ["Invalid input", "Invalid select", "Invalid textarea"])
		expect(screen.getByLabelText(name)).toHaveAttribute("aria-invalid", "false");
	const button = screen.getByRole("button", { name: "Integration" });
	expect(button).toHaveAttribute("aria-pressed", "false");
	fireEvent.click(button);
	expect(button).toHaveAttribute("aria-pressed", "true");
	fireEvent.click(button);
	expect(button).toHaveAttribute("aria-pressed", "false");
});
