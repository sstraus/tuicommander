import { createEffect } from 'solid-js';
import { render } from 'solid-js/web';
import { DictationSettings } from '../../components/SettingsPanel/DictationSettings';
import { dictationStore } from '../../stores/dictation';
import '../../global.css';

render(() => <div style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)"><DictationSettings/></div>, document.getElementById('root'));
createEffect(() => {
  if(dictationStore.state.language !== 'en') return;
  requestAnimationFrame(() => {
    const listen=[...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='Listen');
    if(!listen) throw new Error('Spoken reply picker did not mount');
    listen.closest('[class*="settingRow"]').id='geometry';
    window.controlGeometryReady=true;
  });
});
