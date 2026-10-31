import { render } from 'solid-js/web';
import { Terminal } from '../../components/Terminal/Terminal';
import { terminalsStore } from '../../stores/terminals';
import { acpTranscript } from '../../stores/acpTranscript';
import { chatViewKey } from '../../stores/chatView';
import { EditorView } from '@codemirror/view';
import '../../global.css';

// Browser component fixture: no PTY is created and no backend is attached.
const id = terminalsStore.add({ sessionId: 'focus-browser-2554', cwd: '/repo', repoPath: '/repo', name: 'Session Chat', fontSize: 13, awaitingInput: null });
terminalsStore.update(id, { agentType: 'claude', agentSessionId: 'focus-browser', shellState: 'idle' });
terminalsStore.setActive(id);
terminalsStore.setViewMode(id, 'chat');
window.seedFocusTranscript = () => acpTranscript.restore(chatViewKey('focus-browser-2554'), [
 { id: 'question', kind: 'user', text: 'Review the session Chat keyboard focus.' },
 { id: 'answer', kind: 'agent', text: 'Click this transcript, then type a reply. Selecting this text must still work.' },
]);
render(() => <main style="position:relative;width:100%;height:100vh"><Terminal id={id} cwd="/repo" alwaysVisible /></main>, document.getElementById('root'));
window.focusFixture = () => {
 const cm = EditorView.findFromDOM(document.querySelector('.cm-editor'));
 return { draft: cm.state.doc.toString(), focused: document.activeElement === cm.contentDOM, selection: window.getSelection()?.toString(), active: document.activeElement?.outerHTML.slice(0,200) };
};
