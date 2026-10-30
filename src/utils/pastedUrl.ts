/** Return an augmented paste only when a final web URL needs separation. */
export function spacedUrlPaste(text: string, nextCharacter: string): string | null {
	if (/\s/.test(nextCharacter) || /\s$/.test(text)) return null;
	const token = text.match(/(?:^|\s)(https?:\/\/\S+)$/i)?.[1];
	if (!token) return null;
	try {
		const url = new URL(token);
		if (!url.hostname) return null;
	} catch {
		return null;
	}
	return `${text} `;
}
