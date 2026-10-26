import { render } from 'solid-js/web';
import { SearchBar } from '../../components/shared/SearchBar';
import '../../global.css';
const noop = () => {};
render(() => <SearchBar visible matchIndex={0} matchCount={3} onSearch={noop} onNext={noop} onPrev={noop} onClose={noop} />, document.getElementById('root'));
requestAnimationFrame(() => {document.querySelector('[class*="overlay"]').id='geometry'; window.controlGeometryReady=true;});
