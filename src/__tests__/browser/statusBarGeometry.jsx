import { render } from 'solid-js/web';
import { StatusBar } from '../../components/StatusBar/StatusBar';
import '../../global.css';
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)"><StatusBar fontSize={14} defaultFontSize={14} statusInfo="Ready" onToggleDiff={() => {}} onToggleMarkdown={() => {}} onDictationStart={() => {}} onDictationStop={() => {}} /></section>, document.getElementById('root'));
document.querySelector('#status-bar > div:last-child').dataset.row = 'compact';
window.controlGeometryReady = true;
