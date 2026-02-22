import { createSignal } from "solid-js";

const [locale, setLocale] = createSignal("en");

export { locale, setLocale };

export function t(_key: string, fallback: string, params?: Record<string, string>): string {
  let str = fallback;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      str = str.replace(new RegExp(`\\{${k}\\}`, "g"), v);
    }
  }
  return str;
}
