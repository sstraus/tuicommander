import { render } from 'solid-js/web';
import { IdeLauncher } from '../../components/IdeLauncher/IdeLauncher';
import '../../global.css';
// Native-only component rendered with a synthetic detection reply, never a native launch.
window.__TAURI_INTERNALS__={invoke:async command=>command==='detect_installed_ides'?['vscode','terminal']:undefined};
render(()=><section id="geometry" style="width:360px;max-width:100%;min-height:350px;padding:8px"><IdeLauncher repoPath="/geometry-fixture" runCommand="pnpm dev"/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{document.querySelector('button[title="Choose editor"]').click();requestAnimationFrame(()=>{window.controlGeometryReady=true;});});
