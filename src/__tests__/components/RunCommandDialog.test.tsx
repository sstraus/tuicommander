import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { RunCommandDialog } from "../../components/RunCommandDialog/RunCommandDialog";

describe("RunCommandDialog", () => {
  it("renders dialog when visible", () => {
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand=""
        onClose={() => {}}
        onSaveAndRun={() => {}}
      />
    ));
    const dialog = container.querySelector(".wider");
    expect(dialog).not.toBeNull();
    const heading = container.querySelector("h4");
    expect(heading!.textContent).toBe("Run Command");
  });

  it("returns null when not visible", () => {
    const { container } = render(() => (
      <RunCommandDialog
        visible={false}
        savedCommand=""
        onClose={() => {}}
        onSaveAndRun={() => {}}
      />
    ));
    const dialog = container.querySelector(".wider");
    expect(dialog).toBeNull();
  });

  it("pre-fills input with saved command", () => {
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand="npm run dev"
        onClose={() => {}}
        onSaveAndRun={() => {}}
      />
    ));
    const input = container.querySelector(".monoInput") as HTMLInputElement;
    expect(input).not.toBeNull();
    expect(input.value).toBe("npm run dev");
  });

  it("calls onSaveAndRun with trimmed command on submit", () => {
    const handleSaveAndRun = vi.fn();
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand=""
        onClose={() => {}}
        onSaveAndRun={handleSaveAndRun}
      />
    ));
    const input = container.querySelector(".monoInput") as HTMLInputElement;
    fireEvent.input(input, { target: { value: "  cargo watch  " } });

    const saveBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(saveBtn);

    expect(handleSaveAndRun).toHaveBeenCalledWith("cargo watch");
  });

  it("disables Save & Run when input is empty", () => {
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand=""
        onClose={() => {}}
        onSaveAndRun={() => {}}
      />
    ));
    const saveBtn = container.querySelector(".primaryBtn") as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(true);
  });

  it("calls onClose when cancel button is clicked", () => {
    const handleClose = vi.fn();
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand=""
        onClose={handleClose}
        onSaveAndRun={() => {}}
      />
    ));
    const cancelBtn = container.querySelector(".cancelBtn")!;
    fireEvent.click(cancelBtn);
    expect(handleClose).toHaveBeenCalledOnce();
  });

  it("does not call onSaveAndRun with empty command", () => {
    const handleSaveAndRun = vi.fn();
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand=""
        onClose={() => {}}
        onSaveAndRun={handleSaveAndRun}
      />
    ));
    const input = container.querySelector(".monoInput") as HTMLInputElement;
    fireEvent.input(input, { target: { value: "   " } });

    const saveBtn = container.querySelector(".primaryBtn")!;
    fireEvent.click(saveBtn);

    expect(handleSaveAndRun).not.toHaveBeenCalled();
  });

  it("shows description text", () => {
    const { container } = render(() => (
      <RunCommandDialog
        visible={true}
        savedCommand=""
        onClose={() => {}}
        onSaveAndRun={() => {}}
      />
    ));
    const desc = container.querySelector(".description");
    expect(desc).not.toBeNull();
    expect(desc!.textContent).toContain("Enter a command to run");
  });
});
