import ts from "@typescript/typescript6";

/** Parse JSX, not labels or an inventory maintained by the page author. */
export interface SettingControl {
	file: string;
	line: number;
	tag: string;
	settingId?: string;
	nonSetting?: string;
	exclusion?: string;
	enumSource?: string;
	/** Only backend descriptor IDs or explicit shared-component forwarding are dynamic. */
	dynamic: boolean;
}

const controls = new Set([
	"input",
	"button",
	"select",
	"textarea",
	"Input",
	"Button",
	"ColorSwatch",
	"Slider",
	"ColorInput",
	"Select",
	"Textarea",
	"SettingInput",
	"SettingSelect",
	"SettingToggle",
	"SettingSlider",
	"SettingMultiSelect",
	"SettingAccount",
	"SettingList",
	"BooleanOverride",
	"LaunchInstructionsEditor",
	"KeyComboCapture",
	"ColorSwatchPicker",
]);

export function extractSettingControls(file: string, source: string): SettingControl[] {
	const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
	// Solid's Switch is control flow, not a Settings input. Resolve the UI binding, including aliases.
	const switchBindings = new Set<string>();
	for (const statement of tree.statements) {
		if (!ts.isImportDeclaration(statement) || !ts.isStringLiteral(statement.moduleSpecifier)) continue;
		if (!/\/ui(?:\/Switch)?$/.test(statement.moduleSpecifier.text)) continue;
		const bindings = statement.importClause?.namedBindings;
		if (bindings && ts.isNamedImports(bindings)) {
			for (const binding of bindings.elements)
				if ((binding.propertyName ?? binding.name).text === "Switch") switchBindings.add(binding.name.text);
		}
	}

	const found: SettingControl[] = [];
	function visit(node: ts.Node): void {
		if (
			(ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
			(controls.has(node.tagName.getText(tree)) || switchBindings.has(node.tagName.getText(tree)))
		) {
			const attributes = node.attributes.properties;
			function attribute(name: string): string | undefined {
				const prop = attributes.find((p) => ts.isJsxAttribute(p) && p.name.getText(tree) === name);
				if (!prop || !ts.isJsxAttribute(prop) || !prop.initializer) return undefined;
				if (ts.isStringLiteral(prop.initializer)) return prop.initializer.text;
				if (
					ts.isJsxExpression(prop.initializer) &&
					prop.initializer.expression &&
					ts.isStringLiteral(prop.initializer.expression)
				)
					return prop.initializer.expression.text;
				return undefined;
			}
			let enumSource: string | undefined;
			if (ts.isJsxOpeningElement(node) && ts.isJsxElement(node.parent)) {
				function options(n: ts.Node): void {
					if (
						ts.isCallExpression(n) &&
						n.expression.getText(tree) === "settingOptions" &&
						n.arguments[0] &&
						ts.isStringLiteral(n.arguments[0])
					)
						enumSource = n.arguments[0].text;
					ts.forEachChild(n, options);
				}
				options(node.parent);
			}
			// Resolve finite dynamic row IDs from the production array, not a parallel inventory.
			const idAttribute = attributes.find(
				(p) => ts.isJsxAttribute(p) && ["settingId", "data-setting-id"].includes(p.name.getText(tree)),
			);
			const expression =
				idAttribute &&
				ts.isJsxAttribute(idAttribute) &&
				idAttribute.initializer &&
				ts.isJsxExpression(idAttribute.initializer)
					? idAttribute.initializer.expression
					: undefined;
			const resolvedIds: string[] = [];
			if (
				expression &&
				ts.isConditionalExpression(expression) &&
				ts.isStringLiteral(expression.whenTrue) &&
				ts.isStringLiteral(expression.whenFalse)
			) {
				resolvedIds.push(expression.whenTrue.text, expression.whenFalse.text);
			}
			if (expression && ts.isTemplateExpression(expression) && expression.templateSpans.length === 1) {
				const prefix = expression.head.text;
				const span = expression.templateSpans[0];
				if (ts.isPropertyAccessExpression(span.expression)) {
					const property = span.expression.name.text;
					const binding = span.expression.expression.getText(tree);
					let parent: ts.Node | undefined = node.parent;
					while (parent && !(ts.isArrowFunction(parent) && parent.parameters[0]?.name.getText(tree) === binding))
						parent = parent.parent;
					if (parent && ts.isArrowFunction(parent) && parent.parameters[0]?.name.getText(tree) === binding) {
						let loop: ts.Node | undefined = parent.parent;
						while (loop && !(ts.isJsxElement(loop) && loop.openingElement.tagName.getText(tree) === "For"))
							loop = loop.parent;
						if (loop && ts.isJsxElement(loop)) {
							const each = loop.openingElement.attributes.properties.find(
								(p) => ts.isJsxAttribute(p) && p.name.getText(tree) === "each",
							);
							const name =
								each && ts.isJsxAttribute(each) && each.initializer && ts.isJsxExpression(each.initializer)
									? each.initializer.expression?.getText(tree)
									: undefined;
							function arrays(n: ts.Node): void {
								if (
									ts.isVariableDeclaration(n) &&
									n.name.getText(tree) === name &&
									n.initializer &&
									ts.isArrayLiteralExpression(n.initializer)
								) {
									for (const item of n.initializer.elements)
										if (ts.isObjectLiteralExpression(item)) {
											const key = item.properties.find(
												(p) => ts.isPropertyAssignment(p) && p.name.getText(tree) === property,
											);
											if (key && ts.isPropertyAssignment(key) && ts.isStringLiteral(key.initializer))
												resolvedIds.push(prefix + key.initializer.text + span.literal.text);
										}
								}
								ts.forEachChild(n, arrays);
							}
							arrays(tree);
						}
					}
				}
			}
			for (const settingId of resolvedIds.length
				? resolvedIds
				: [attribute("settingId") ?? attribute("data-setting-id")])
				found.push({
					file,
					line: tree.getLineAndCharacterOfPosition(node.getStart(tree)).line + 1,
					tag: node.tagName.getText(tree),
					settingId,
					nonSetting: attribute("data-non-setting"),
					exclusion: attribute("data-setting-exclusion"),
					enumSource,
					dynamic: attributes.some((p) => {
						if (
							!ts.isJsxAttribute(p) ||
							!["settingId", "data-setting-id"].includes(p.name.getText(tree)) ||
							!p.initializer ||
							!ts.isJsxExpression(p.initializer) ||
							!p.initializer.expression
						)
							return false;
						const expression = p.initializer.expression.getText(tree);
						return (
							(file.endsWith("/AllSettingsTab.tsx") && expression === "field().id as SettingId") ||
							(file.endsWith("/PluginSettingsTab.tsx") && ["props.field.id", "field.id"].includes(expression)) ||
							(["/SettingFields.tsx", "/RepoWorktreeTab.tsx", "/LaunchInstructionsEditor.tsx"].some((suffix) =>
								file.endsWith(suffix),
							) &&
								expression === "props.settingId")
						);
					}),
				});
		}
		ts.forEachChild(node, visit);
	}
	visit(tree);
	return found;
}

export function missingControlIdentities(
	controls: SettingControl[],
	exclusions: Record<string, string> = {},
): SettingControl[] {
	return controls.filter(
		(control) =>
			!control.settingId &&
			!control.nonSetting?.trim() &&
			!control.dynamic &&
			!(control.exclusion && exclusions[control.exclusion]?.trim()),
	);
}

export function missingCatalogueControls(
	ids: readonly string[],
	controls: SettingControl[],
	exclusions: Record<string, string>,
): string[] {
	const bound = new Set(controls.flatMap((control) => (control.settingId ? [control.settingId] : [])));
	return ids.filter((id) => !bound.has(id) && !exclusions[id]?.trim());
}
