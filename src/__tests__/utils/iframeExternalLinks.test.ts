import { Window } from "happy-dom";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";
import { IFRAME_EXTERNAL_LINK_SCRIPT } from "../../utils/iframeExternalLinks";

afterEach(() => vi.restoreAllMocks());

it("sends a clicked HTTPS link to the preview host and leaves a fragment inside the frame", () => {
	// Own the frame's actual Window rather than Vitest's global window proxy:
	// happy-dom dispatches anchor navigation through the element's owning Window.
	const preview = new Window({ url: "https://preview.example/" });
	onTestFinished(() => preview.happyDOM.close());
	const open = vi.spyOn(preview, "open").mockReturnValue(null);
	const doc = preview.document;
	const postMessage = vi.fn();
	const body = IFRAME_EXTERNAL_LINK_SCRIPT.slice(
		IFRAME_EXTERNAL_LINK_SCRIPT.indexOf(">") + 1,
		IFRAME_EXTERNAL_LINK_SCRIPT.lastIndexOf("</script>"),
	);
	new Function("document", "parent", body)(doc, { postMessage });
	const external = doc.createElement("a");
	external.href = "https://example.org/reference";
	doc.body.append(external);
	const click = new preview.MouseEvent("click", { bubbles: true, cancelable: true });
	external.dispatchEvent(click);
	expect(click.defaultPrevented).toBe(true);
	expect(postMessage).toHaveBeenCalledWith(
		{ type: "tuic:preview-open-url", url: "https://example.org/reference" },
		"*",
	);
	expect(open).not.toHaveBeenCalled();
	postMessage.mockClear();
	const fragment = doc.createElement("a");
	fragment.href = "#section";
	doc.body.append(fragment);
	const fragmentClick = new preview.MouseEvent("click", { bubbles: true, cancelable: true });
	fragment.dispatchEvent(fragmentClick);
	expect(fragmentClick.defaultPrevented).toBe(false);
	expect(postMessage).not.toHaveBeenCalled();
	expect(open).toHaveBeenCalledExactlyOnceWith("https://preview.example/#section", "_self", "");
});
