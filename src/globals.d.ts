/** App version injected by Vite from tauri.conf.json */
declare const __APP_VERSION__: string;

/** Git commit hash injected by Vite at build time — used for PWA version checks */
declare const __BUILD_GIT_HASH__: string;

/** CSS Modules — import styles from "./Foo.module.css" */
declare module "*.module.css" {
  const classes: Record<string, string>;
  export default classes;
}
