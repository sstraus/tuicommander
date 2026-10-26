import { render } from 'solid-js/web';
import { PrDetailPopover } from '../../components/PrDetailPopover/PrDetailPopover';
import { githubStore } from '../../stores/github';
import '../../global.css';
// Recorded shape from the component test fixture; no remote GitHub operation is exercised.
window.__TAURI_INTERNALS__={invoke:async command=>command==='get_pr_action_state'?{}:command==='get_pr_check_details'?[]:null};
githubStore.updateRepoData('/fixture',[{number:42,title:'Shared control geometry',branch:'feature',state:'OPEN',url:'https://github.com/fixture/repo/pull/42',head_ref_oid:'a'.repeat(40),base_ref_name:'main',author:'Fixture',commits:1,additions:10,deletions:2,labels:[],mergeable:'MERGEABLE',merge_state_label:null,review_state_label:null,created_at:null,updated_at:null,checks:{passed:0,failed:0,pending:0,total:0}}]);
render(()=><PrDetailPopover repoPath="/fixture" branch="feature" anchor="top" onClose={()=>{}}/>,document.getElementById('root'));
requestAnimationFrame(()=>{
 const request=[...document.querySelectorAll('button')].find(button=>button.textContent==='Request changes');
 if(!request)throw new Error('PR actions did not mount');
 request.click();
 request.closest('[class*="popover"]').id='geometry';
 window.controlGeometryReady=true;
});
