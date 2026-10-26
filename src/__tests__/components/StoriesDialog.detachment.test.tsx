import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { StoriesDialog } from "../../components/StoriesDialog/StoriesDialog";
import { detachPanel, reattachPanel } from "../../panelRouter";
import { registerModal } from "../../stores/modalStack";

vi.mock("../../invoke", () => ({ invoke: vi.fn().mockResolvedValue({ type: "plans", value: [] }) }));
vi.mock("../../stores/modalStack", () => ({ registerModal: vi.fn() }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));
vi.mock("../../panelRouter", () => ({ detachPanel: vi.fn(), reattachPanel: vi.fn(), closePanel: vi.fn() }));
vi.mock("../../transport", () => ({ isTauri: () => true, HttpRpcError: class extends Error {} }));

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});

it("catches a Stories detach button bypassing the shared panel router", () => {
	const { getByTitle } = render(() => <StoriesDialog project="/fixture" onClose={() => {}} />);
	fireEvent.click(getByTitle("Open in separate window"));
	expect(detachPanel).toHaveBeenCalledWith("stories");
});

it("catches a detached Stories window retaining modal dismissal or losing bring-back controls", () => {
	const onClose = vi.fn();
	const { getByRole, getByTitle } = render(() => (
		<StoriesDialog project="/fixture" mode="detached" onClose={onClose} />
	));
	const dialog = getByRole("dialog");
	expect(dialog.getAttribute("aria-modal")).toBeNull();
	expect(registerModal).not.toHaveBeenCalled();
	fireEvent.click(dialog.parentElement!);
	expect(onClose).not.toHaveBeenCalled();
	fireEvent.click(getByTitle("Bring back to main window"));
	expect(reattachPanel).toHaveBeenCalledWith("stories");
});
