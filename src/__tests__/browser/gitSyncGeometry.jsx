import { render } from 'solid-js/web';
import { SyncRow } from '../../components/GitPanel/SyncRow';
import '../../global.css';
render(()=><section id="geometry" style="width:min(700px,100%);padding:16px;background:var(--bg-secondary)"><SyncRow repoPath="/fixture" status={{branch:'main',upstream:'origin/main',ahead:1,behind:2,stash_count:1,staged:[],unstaged:[],untracked:[]}}/></section>,document.getElementById('root'));
const pull=[...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='Pull');
if(!pull)throw new Error('Sync actions did not mount');
pull.parentElement.dataset.row='compact';
window.controlGeometryReady=true;
