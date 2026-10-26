import { render } from 'solid-js/web';
import { ProviderRecoveryStatus } from '../../components/StatusBar/ProviderRecoveryStatus';
import { Select } from '../../components/ui';
import '../../global.css';
render(() => <section id="geometry" style="padding:16px;background:var(--bg-secondary)"><div data-row="compact" style="display:flex;align-items:center;gap:8px;flex-wrap:wrap"><ProviderRecoveryStatus sessionId="fixture" recovery={{phase:'waiting',attempt:3,next_attempt_ms:null}} /><Select compact aria-label="Comparison"><option>Ready</option></Select></div></section>, document.getElementById('root'));
window.controlGeometryReady = true;
