import { render } from 'solid-js/web';
import { Composer } from '../../components/AIChatPanel/Composer';
import { aiChatDraft } from '../../components/AIChatPanel/draft';
import { ComposePanel } from '../../components/ComposePanel/ComposePanel';
import '../../global.css';
// Local component state only: no ACP connection or terminal input.
const noop = () => {};
const chat = { sessionId: () => '', queuedPrompts: () => [], busy: () => false, configOptions: () => [], cancel: noop, send: noop };
render(() => <div id="geometry">
  <ComposePanel isOpen={() => true} initialText={() => 'Fixture draft'} pinned={() => true}
    onClose={noop} onSend={noop} onEnqueue={noop} canEnqueue={() => true} queuedCount={() => 0}
    onClearQueue={noop} onLoadQueue={async () => []} onRemoveQueued={noop} onTogglePin={noop}
    onDismiss={noop} focusRequest={() => 0} />
  <Composer chat={chat} />
</div>, document.getElementById('root'));
aiChatDraft.set('Fixture draft');
requestAnimationFrame(() => { window.controlGeometryReady = true; });
