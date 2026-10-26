import { render } from 'solid-js/web';
import { NewDraftForm, DraftCommentView } from '../../components/PrDiffTab/DraftCommentView';
import { ReviewSubmitBar } from '../../components/PrDiffTab/ReviewSubmitBar';
import { ReviewThreadView } from '../../components/PrDiffTab/ReviewThreadView';
import { recordedPr } from '../../components/PrDiffTab/__tests__/recorded';
import '../../global.css';
// Real recorded thread data, isolated session methods: backend mutations are not exercised.
const {threads}=recordedPr(340803);
const session={drafts:()=>[],headMoved:()=>false,submitting:()=>false,error:()=>null,busyThread:()=>null,submit:async()=>true,reply:async()=>true,setResolved:async()=>{},loadMoreComments:async()=>{}};
render(()=><section id="geometry" style="width:min(700px,100%);background:var(--bg-secondary)"><NewDraftForm disabled={false} onAdd={()=>{}} onClose={()=>{}}/><DraftCommentView draft={{path:'a.ts',line:1,body:'Pending comment'}} onRemove={()=>{}}/><ReviewSubmitBar session={session}/><ReviewThreadView thread={threads[0]} session={session}/></section>,document.getElementById('root'));
window.controlGeometryReady=true;
