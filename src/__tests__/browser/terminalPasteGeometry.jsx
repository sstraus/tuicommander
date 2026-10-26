import { render } from 'solid-js/web';
import { TerminalUserText } from '../../components/Terminal/chatPresentation';
import { Select } from '../../components/ui';
import '../../global.css';

render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)">
  <TerminalUserText text={'Inspect:\nBash(cat result.txt)\n  ⎿ complete\n  next\n  exit=0'} prose={text => <p>{text}</p>}/>
  <Select aria-label="Reference status"><option>Ready</option></Select>
</section>, document.getElementById('root'));
window.controlGeometryReady = true;
