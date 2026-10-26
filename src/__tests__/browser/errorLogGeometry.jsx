import { render } from 'solid-js/web';
import { ErrorLogPanel } from '../../components/ErrorLogPanel/ErrorLogPanel';
import { errorLogStore } from '../../stores/errorLog';
import { appLogger } from '../../stores/appLogger';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async()=>undefined};
// Synthetic frontend log entries; no native log transport is exercised.
appLogger.error('app','Geometry fixture');
errorLogStore.open();
render(()=><ErrorLogPanel/>,document.getElementById('root'));
requestAnimationFrame(()=>{document.querySelector('h3').parentElement.parentElement.id='geometry';window.controlGeometryReady=true;});
