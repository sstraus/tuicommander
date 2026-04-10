/**
 * Sanitize an SVG icon string to prevent XSS.
 *
 * Only allows safe SVG elements and attributes. Strips scripts,
 * event handlers, and anything outside the SVG allowlist.
 */

const ALLOWED_TAGS = new Set([
  "svg", "path", "circle", "ellipse", "line", "polygon", "polyline",
  "rect", "g", "defs", "use", "symbol", "title", "desc",
  "clippath", "mask", "pattern", "lineargradient", "radialgradient", "stop",
]);

const ALLOWED_ATTRS = new Set([
  "viewbox", "xmlns", "fill", "stroke", "stroke-width", "stroke-linecap",
  "stroke-linejoin", "d", "cx", "cy", "r", "rx", "ry", "x", "y",
  "x1", "y1", "x2", "y2", "width", "height", "points", "transform",
  "opacity", "fill-opacity", "stroke-opacity", "fill-rule", "clip-rule",
  "id", "class", "style", "offset", "stop-color", "stop-opacity",
  "gradientunits", "gradienttransform", "patternunits", "patterntransform",
  "clip-path", "mask", "href",
]);

/**
 * Sanitize an SVG string, keeping only safe tags/attributes.
 * Returns empty string if the input is not a valid SVG fragment.
 */
export function sanitizeSvgIcon(html: string | undefined | null): string {
  if (!html || typeof html !== "string") return "";

  const trimmed = html.trim();
  if (!trimmed.startsWith("<svg") && !trimmed.startsWith("<SVG")) return "";

  const parser = new DOMParser();
  const doc = parser.parseFromString(trimmed, "image/svg+xml");

  // DOMParser returns a parsererror element on invalid XML
  if (doc.querySelector("parsererror")) return "";

  const root = doc.documentElement;
  if (root.tagName.toLowerCase() !== "svg") return "";

  sanitizeNode(root);
  return root.outerHTML;
}

function sanitizeNode(node: Element): void {
  // Remove disallowed children
  const children = Array.from(node.children);
  for (const child of children) {
    if (!ALLOWED_TAGS.has(child.tagName.toLowerCase())) {
      child.remove();
      continue;
    }
    sanitizeAttributes(child);
    sanitizeNode(child);
  }
}

function sanitizeAttributes(el: Element): void {
  const attrs = Array.from(el.attributes);
  for (const attr of attrs) {
    const name = attr.name.toLowerCase();
    // Remove event handlers and non-allowlisted attributes
    if (name.startsWith("on") || !ALLOWED_ATTRS.has(name)) {
      el.removeAttribute(attr.name);
    }
  }
}
