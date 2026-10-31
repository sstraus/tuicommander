import { render } from 'solid-js/web';
import '../../global.css';
import { Transcript } from '../../components/AIChatPanel/Transcript';
const prompt = "ok riprendiamo, c'erano lavori in corso e dobbiamo completare tutte le modifiche senza perdere il contesto. ".repeat(8);
const entries = [
  {id:'empty-plan',kind:'plan',entries:[]},
  { id:'prompt',kind:'user',text:prompt },
  { id:'answer',kind:'agent',text:'This paragraph comes from the structured transcript, not the PTY grid. '.repeat(4) + 'Use `--session` to continue the conversation. '.repeat(7) },
  { id:'path',kind:'user',text:'A'.repeat(400) },
  { id:'multiline',kind:'user',text:'Keep this intentional newline.\nAnd this second line.' },
];
const width = Number(new URLSearchParams(location.search).get('width') || 960);
render(() => <main style={{width:width+'px',height:'840px',display:'flex','flex-direction':'column',margin:'16px'}}><Transcript entries={()=>entries} busy={()=>false} terminalAgent="claude" emptyMessage="Empty" observeToolDuration={false}/></main>,document.getElementById('root'));
setTimeout(()=>{
 const row=document.querySelector('[data-terminal-chat]');
 const box=e=>{const r=e.getBoundingClientRect();return {left:r.left,right:r.right,width:r.width,height:r.height};};
 const users=[...document.querySelectorAll('[aria-label="Copy user message"]')].map(copy=>{
  const bubble=copy.parentElement;
  const range=document.createRange();range.selectNodeContents(bubble);range.setEndBefore(copy);
  return {box:box(bubble),textRects:[...range.getClientRects()].map(box=>({left:box.left,right:box.right,top:box.top})),scrollWidth:bubble.scrollWidth,clientWidth:bubble.clientWidth,text:bubble.textContent};
 });
 const codes=[...document.querySelectorAll('#markdown-content p code')].map(code=>({text:code.textContent,rects:code.getClientRects().length,box:box(code)}));
 const result={width,row:box(row),users,codes,sourceHasNewline:prompt.includes('\n')};
 const output=document.createElement('pre');output.id='geometry-result';output.textContent=JSON.stringify(result);output.style.display='none';document.body.append(output);
},1800);
