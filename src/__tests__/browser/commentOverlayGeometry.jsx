import { render } from 'solid-js/web';
import { CommentOverlay } from '../../components/MarkdownTab/CommentOverlay';
import '../../global.css';

// Actual overlay DOM with in-memory save/delete ports; no source files are written.
render(() => <section id="geometry" style="height:650px;padding:60px 40px;user-select:text"><div id="block"><p data-comment-source-start="0" data-comment-source-end="15">Comment a block</p></div><div id="inline" style="margin-top:90px"><p style="user-select:text;-webkit-user-select:text">Select this text</p></div><div id="existing" style="margin-top:90px"><span class="tweak-highlight" data-tweak-id="c_geometry" data-tweak-comment="Review this wording" data-tweak-at="2026-09-27T12:00:00.000Z">Existing comment</span></div></section>, document.getElementById('root'));
for (const id of ['block', 'inline', 'existing']) {
 const content = document.getElementById(id);
 render(() => <CommentOverlay contentRef={content} onSave={()=>true} onDelete={()=>true}/>, content);
}
const block = document.querySelector('#block p').getBoundingClientRect();
document.getElementById('geometry').dispatchEvent(new MouseEvent('mousemove', {bubbles:true,clientX:block.left-16,clientY:block.top+5}));
const text = document.querySelector('#inline p').firstChild;
const range = document.createRange();range.selectNodeContents(text);
window.getSelection().removeAllRanges();window.getSelection().addRange(range);
document.getElementById('inline').dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));
window.getSelection().removeAllRanges();
document.querySelector('#existing span').click();
await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
// Portal remains the real production subtree; include it in the screenshot/measurement region.
for (const element of document.querySelectorAll('body > div')) {
 if (element.id !== 'root') document.getElementById('geometry').append(element);
}
const buttons = document.querySelectorAll('#geometry [data-form-control]:not(textarea)');
if (buttons.length !== 5) throw new Error(`Expected both comment icons and three edit actions, got ${buttons.length}: ${[...buttons].map(b=>b.getAttribute("aria-label")||b.textContent).join(",")}`);
const textarea = document.querySelector('#geometry textarea');
if (!textarea?.hasAttribute('data-form-control') || textarea.getBoundingClientRect().height < (window.innerWidth <= 600 ? 44 : 108)) throw new Error('Missing shared multiline composer');
window.controlGeometryReady=true;
