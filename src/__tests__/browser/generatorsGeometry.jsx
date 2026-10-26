import { render } from 'solid-js/web';
import { GeneratorsModal } from '../../components/GeneratorsModal/GeneratorsModal';
import '../../global.css';
// Synthetic host response, not evidence about the cryptographic generator backend.
window.__TAURI_INTERNALS__ = {invoke: async command => command === 'generate_value' ? {value:'fixture-output',extra:'fixture-public'} : undefined};
render(() => <GeneratorsModal onClose={() => {}} />, document.getElementById('root'));
requestAnimationFrame(async () => {
 const modal = document.querySelector('nav').parentElement.parentElement;
 modal.id = 'geometry';
 [...modal.querySelectorAll('button')].find(button => button.textContent === 'Nano ID').click();
 await Promise.all(document.getAnimations().map(animation => animation.finished));
 window.controlGeometryReady = true;
});
