import { render } from 'solid-js/web';
import { BoardLane } from '../../components/Tasks/board/BoardLane';
import { Select } from '../../components/ui/Select';
import { detail } from '../../components/Tasks/detail/__tests__/detailFixture';
import '../../global.css';
const columns = [{id:'todo',name:'To do'},{id:'done',name:'Done'}];
const card = {item:detail().item,moves:[]};
render(() => <section id="geometry" style="padding:16px;height:440px;background:var(--bg-primary)">
  <Select aria-label="Status"><option>Ready</option></Select>
  <BoardLane column={columns[0]} columns={columns} cards={[card]} target={false} hasMore pending={()=>false} canMove={()=>true} onMove={()=>{}} onPointerDown={()=>{}} onOpen={()=>{}}/>
</section>,document.getElementById('root'));
while (!document.querySelector('[aria-label="Move DEMO-1"]')) await new Promise(resolve=>requestAnimationFrame(resolve));
window.controlGeometryReady=true;
