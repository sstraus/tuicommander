import { render } from 'solid-js/web';
import { DictationToast } from '../../components/DictationToast/DictationToast';
import { dictationStore } from '../../stores/dictation';
import '../../global.css';
// Read snapshots only. No microphone arm, native speech or OS device is invoked.
window.__TAURI_INTERNALS__={invoke:async command=>{
 if(command==='get_hands_free_status')return {armed:true,listening:true,phase:'waiting',sessionId:'fixture',owner:'desktop',generation:1,pendingText:null,holdBackMs:1500,error:null};
 if(command==='get_speech_status')return {speaking:true,paused:false};
 if(command==='get_dictation_status')return {audio_level:0};
 return {};
}};
render(()=><section><DictationToast/></section>,document.getElementById('root'));
await dictationStore.refreshHandsFree();
await dictationStore.refreshSpeechStatus();
requestAnimationFrame(async()=>{
 const toast=document.querySelector('[data-exiting]');
 toast.id='geometry';
 await Promise.all(toast.getAnimations().map(animation=>animation.finished));
 window.controlGeometryReady=true;
});

window.geometryIconLabels=["Pause reply", "Stop reply", "Spoken replies"];
