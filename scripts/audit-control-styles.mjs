// Feature CSS must not redefine the geometry of shared controls.
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import ts from "@typescript/typescript6";

// Vite owns the CSS parser already used by the frontend build.
const require = createRequire(import.meta.resolve("vite"));
const postcss = require("postcss");
const purgecssRequire = createRequire(createRequire(import.meta.resolve("vite-plugin-purgecss")).resolve("purgecss"));
const parseSelector = purgecssRequire("postcss-selector-parser");
const geometry = /^(?:(?:min-|max-)?(?:height|block-size)|padding(?:-.+)?|border(?:-.+)?)$/;
const controls = new Set(["button", "select", "input", "textarea", "Input", "Select", "Textarea", "SegmentedControl", "Button", "Switch", "Slider", "ColorInput", "ColorSwatch"]);

export function sourceFiles(root = "src") {
	return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
		const file = path.join(root, entry.name).split(path.sep).join("/");
		if (entry.isDirectory()) return sourceFiles(file);
		return [file];
	});
}

/** Map CSS-module classes actually attached to controls, including shared primitives. */
export function controlClasses(files, read = (file) => readFileSync(file, "utf8")) {
	const result = new Map();
	for (const file of files.filter((file) => file.endsWith(".tsx") && !file.includes("/__tests__/"))) {
		const tree = ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
		const imports = new Map();
		for (const statement of tree.statements) {
			if (!ts.isImportDeclaration(statement) || !ts.isStringLiteral(statement.moduleSpecifier)) continue;
			if (!statement.moduleSpecifier.text.endsWith(".module.css")) continue;
			const name = statement.importClause?.name?.text;
			if (name) imports.set(name, path.posix.normalize(path.posix.join(path.posix.dirname(file), statement.moduleSpecifier.text)));
		}
		function visit(node) {
			if ((ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) && controls.has(node.tagName.getText(tree))) {
				for (const attribute of node.attributes.properties) {
					if (!ts.isJsxAttribute(attribute) || !["class", "classList"].includes(attribute.name.getText(tree))) continue;
					function findClass(child) {
						if (ts.isPropertyAccessExpression(child) && ts.isIdentifier(child.expression)) {
							const css = imports.get(child.expression.text);
							if (css) {
								if (!result.has(css)) result.set(css, new Set());
								result.get(css).add(child.name.text);
							}
						}
						ts.forEachChild(child, findClass);
					}
					findClass(attribute);
				}
			}
			ts.forEachChild(node, visit);
		}
		visit(tree);
	}
	return result;
}

export function auditCss(file, css, classes = new Set()) {
	const rows = [];
	postcss.parse(css, { from: file }).walkRules((rule) => {
		// Parse target compounds so :is/:where and CSS-module :global cannot
		// hide controls, while :has(input) and input + .slider remain containers.
		function targetsControl(selector, allowNesting = true, specialized = false) {
			const lastCombinator = selector.nodes.findLastIndex((node) => node.type === "combinator");
			const target = selector.nodes.slice(lastCombinator + 1);
			// Only explicitly marked native button rows/cards are layout, not form actions.
			if (target.some((node) => node.type === "tag" && node.value === "button")
				&& target.some((node) => node.type === "attribute" && node.attribute === "data-control-layout"
					&& node.operator === "=" && ["row", "card"].includes(node.value))) return false;
			if (specialized
				&& target.some((node) => node.type === "tag" && node.value.toLowerCase() === "input")
				&& target.some((node) => node.type === "attribute" && node.attribute.toLowerCase() === "type"
					&& node.operator === "=" && ["checkbox", "range", "color"].includes(node.value?.toLowerCase()))) return true;
			return target.some((node) => {
				if (node.type === "tag") return !specialized && controls.has(node.value);
				if (node.type === "class") return !specialized && classes.has(node.value);
				if (node.type === "pseudo" && [":is", ":where", ":global", ":local"].includes(node.value)) return node.nodes.some((selector) => targetsControl(selector, allowNesting, specialized));
				if (allowNesting && node.type === "nesting" && rule.parent?.type === "rule") {
					for (let parent = rule.parent; parent?.type === "rule"; parent = parent.parent) {
						const selectors = parseSelector().astSync(parent.selector);
						if (selectors.nodes.some((selector) => targetsControl(selector, false, specialized))) return true;
					}
				}
				return false;
			});
		}
		const selectors = parseSelector().astSync(rule.selector).nodes;
		const matches = selectors.some((selector) => targetsControl(selector));
		if (!matches) return;
		const specialized = selectors.some((selector) => targetsControl(selector, true, true));
		const declarations = rule.nodes.filter((node) => node.type === "decl" && (geometry.test(node.prop.toLowerCase()) || (specialized && /^(?:min-|max-)?(?:width|inline-size)$/.test(node.prop.toLowerCase())))).map((node) => `${node.prop.toLowerCase()}: ${node.value}${node.important ? " !important" : ""}`);
		if (!declarations.length) return;
		const context = [];
		for (let parent = rule.parent; parent?.type === "atrule"; parent = parent.parent) context.unshift(`@${parent.name} ${parent.params}`);
		const selector = rule.selector.replace(/\s+/g, " ");
		rows.push({ file, line: rule.source.start.line, selector, declarations, signature: JSON.stringify([context, selector, declarations]) });
	});
	return rows;
}

export function auditControls(files = sourceFiles(), read = (file) => readFileSync(file, "utf8")) {
	const classes = controlClasses(files, read);
	return files.filter((file) => file.endsWith(".module.css") && !file.startsWith("src/components/ui/")).flatMap((file) => auditCss(file, read(file), classes.get(file)));
}
