/**
 * Capture an iframe's visible content as a WebP base64 string using the
 * SVG foreignObject → canvas pipeline. No external dependencies.
 *
 * Requires same-origin access (srcdoc iframes with allow-same-origin sandbox).
 * Returns null if the iframe content is not accessible.
 */
export async function captureIframeAsWebp(iframe: HTMLIFrameElement, quality = 0.75): Promise<string | null> {
	// URL-mode iframes are cross-origin — canvas toBlob throws SecurityError.
	if (iframe.src && !iframe.srcdoc) return null;

	const doc = iframe.contentDocument;
	if (!doc?.documentElement) return null;

	const width = iframe.clientWidth || 1024;
	const height = iframe.clientHeight || 768;

	const html = new XMLSerializer().serializeToString(doc.documentElement);
	const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
		<foreignObject width="100%" height="100%">
			${html}
		</foreignObject>
	</svg>`;

	const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
	const url = URL.createObjectURL(blob);

	try {
		const img = new Image();
		img.width = width;
		img.height = height;
		await new Promise<void>((resolve, reject) => {
			img.onload = () => resolve();
			img.onerror = () => reject(new Error("SVG foreignObject render failed"));
			img.src = url;
		});

		const canvas = document.createElement("canvas");
		canvas.width = width;
		canvas.height = height;
		const ctx = canvas.getContext("2d");
		if (!ctx) return null;
		ctx.drawImage(img, 0, 0, width, height);

		const webpBlob = await new Promise<Blob | null>((resolve) =>
			canvas.toBlob((b) => resolve(b), "image/webp", quality),
		);
		if (!webpBlob) return null;

		const buffer = await webpBlob.arrayBuffer();
		const bytes = new Uint8Array(buffer);
		let binary = "";
		for (let i = 0; i < bytes.length; i++) {
			binary += String.fromCharCode(bytes[i]);
		}
		return btoa(binary);
	} finally {
		URL.revokeObjectURL(url);
	}
}
