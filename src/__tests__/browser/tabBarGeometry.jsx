import { render } from 'solid-js/web';
import { TabBar } from '../../components/TabBar/TabBar';
import { terminalsStore } from '../../stores/terminals';
import { globalWorkspaceStore } from '../../stores/globalWorkspace';
import s from '../../components/TabBar/TabBar.module.css';
import '../../global.css';
import '../../styles.css';

const ids = ['Rename this terminal', 'Promoted terminal', 'A long terminal name that makes the tab strip overflow', 'Another terminal'].map(name => terminalsStore.add({name, sessionId:null, fontSize:14, cwd:null}));
globalWorkspaceStore.promote(ids[1]);
render(() => <section id="geometry" style="padding:16px;width:100%;box-sizing:border-box;background:var(--bg-secondary)">
  <div id="tab-bar"><TabBar onTabSelect={() => {}} onTabClose={() => {}} onCloseOthers={() => {}} onCloseToRight={() => {}} onNewTab={() => {}} /></div>
</section>, document.getElementById('root'));
document.querySelector(`[data-tab-id="${ids[0]}"]`).dispatchEvent(new MouseEvent('dblclick', {bubbles:true}));
window.controlGeometryRows = () => {
  const strip = document.querySelector('#tab-bar').getBoundingClientRect();
  const compact = [...document.querySelectorAll(`.${s.tabClose}, .${s.globeIcon}, input`)];
  const edge = [...document.querySelectorAll(`.${s.scrollArrow}, .${s.newBtn}`)];
  for (const control of [...compact, ...edge]) {
    const rect = control.getBoundingClientRect();
    if (rect.top < strip.top || rect.bottom > strip.bottom) throw new Error('Shared control clipped by tab strip');
  }
  if (window.innerWidth <= 600) {
    const tabs = document.querySelector(`.${s.tabs}`);
    if (tabs.scrollWidth <= tabs.clientWidth) throw new Error('Fixture must exercise real overflow');
  }
  const measure = element => ({size:window.controlGeometryDensity(element),tag:element.tagName, name:element.title || element.value, height:element.getBoundingClientRect().height});
  return [{size:'compact', heights:compact.map(measure)}, {size:'stretch', containerHeight:document.querySelector('#tab-bar').firstElementChild.getBoundingClientRect().height, heights:edge.map(measure)}];
};
window.controlGeometryReady = true;
