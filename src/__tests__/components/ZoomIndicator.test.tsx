import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { ZoomIndicator } from "../../components/ui/ZoomIndicator";

describe("ZoomIndicator", () => {
	it("renders 100% when fontSize equals defaultFontSize", () => {
		const { container } = render(() => <ZoomIndicator fontSize={14} defaultFontSize={14} />);
		const span = container.querySelector("[data-testid='zoom-indicator']");
		expect(span).not.toBeNull();
		expect(span!.textContent).toBe("100%");
	});

	it("renders 150% when fontSize is 1.5x defaultFontSize", () => {
		const { container } = render(() => <ZoomIndicator fontSize={21} defaultFontSize={14} />);
		const span = container.querySelector("[data-testid='zoom-indicator']");
		expect(span!.textContent).toBe("150%");
	});

	it("renders 50% when fontSize is half of defaultFontSize", () => {
		const { container } = render(() => <ZoomIndicator fontSize={7} defaultFontSize={14} />);
		const span = container.querySelector("[data-testid='zoom-indicator']");
		expect(span!.textContent).toBe("50%");
	});

	it("renders 200% for double fontSize", () => {
		const { container } = render(() => <ZoomIndicator fontSize={28} defaultFontSize={14} />);
		const span = container.querySelector("[data-testid='zoom-indicator']");
		expect(span!.textContent).toBe("200%");
	});

	it("rounds percentage to nearest integer", () => {
		const { container } = render(() => <ZoomIndicator fontSize={10} defaultFontSize={14} />);
		const span = container.querySelector("[data-testid='zoom-indicator']");
		// 10/14 = 0.7142... -> 71%
		expect(span!.textContent).toBe("71%");
	});
});
