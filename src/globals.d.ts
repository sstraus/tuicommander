/** App version injected by Vite from tauri.conf.json */
declare const __APP_VERSION__: string;

/** CSS Modules — import styles from "./Foo.module.css" */
declare module "*.module.css" {
  const classes: Record<string, string>;
  export default classes;
}
