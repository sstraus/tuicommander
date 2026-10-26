import { render } from 'solid-js/web';
import { GithubOpsDashboard } from '../../components/GithubOpsDashboard/GithubOpsDashboard';
import { githubOpsStore } from '../../stores/githubOps';
import '../../global.css';
// Deterministic frontend event state; this fixture performs no scan or GitHub mutation.
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
githubOpsStore.handleEvent('proposals-ready',{repo_path:'/geometry-dashboard',payload:{proposals:[{title:'Improve coverage',summary:'Cover the boundary',issue_title:'Add coverage',issue_body:'Cover the boundary',labels:['testing'],impact:'medium',effort:'small'}]}});
render(()=><section id="geometry"><GithubOpsDashboard repoPath="/geometry-dashboard"/></section>,document.getElementById('root'));
requestAnimationFrame(()=>{window.controlGeometryReady=true;});
