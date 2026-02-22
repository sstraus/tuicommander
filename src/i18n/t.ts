import { createSignal, createMemo } from "solid-js";

type Dict = Record<string, string>;

const [locale, setLocale] = createSignal("en");
const [dicts, setDicts] = createSignal<Record<string, Dict>>({});
const dict = createMemo(() => dicts()[locale()] ?? {});

export { locale, setLocale };

export function registerLocale(code: string, translations: Dict): void {
  setDicts((prev) => ({ ...prev, [code]: translations }));
}

export function t(key: string, fallback: string, params?: Record<string, string>): string {
  let str = dict()[key] ?? fallback;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      str = str.replace(new RegExp(`\\{${k}\\}`, "g"), v);
    }
  }
  return str;
}
