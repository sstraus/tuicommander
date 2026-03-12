/** Replace `{varName}` placeholders with values from `vars`.
 *  Null values become empty string. Unknown variables are left as-is. */
export function interpolateTemplate(
  template: string,
  vars: Record<string, string | null>,
): string {
  return template.replace(/\{(\w+)\}/g, (match, key: string) =>
    key in vars ? (vars[key] ?? "") : match,
  );
}
