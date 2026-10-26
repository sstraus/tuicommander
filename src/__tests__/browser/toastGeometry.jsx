import { render } from 'solid-js/web';
import { ToastList } from '../../components/ToastContainer/ToastList';
import { toastsStore } from '../../stores/toasts';
import '../../global.css';

// Local, sticky presentation state; actions never navigate or contact a backend.
toastsStore.add('Build complete','The terminal is ready.','info',false,{label:'Open terminal',onClick:()=>{}},0);
render(() => <ToastList onDismiss={()=>{}} repoName={()=>'Geometry'} repoAction={()=>({label:'Go to repo',onClick:()=>false})}/>,document.getElementById('root'));
document.querySelector('[role="button"]').id='geometry';
window.controlGeometryReady=true;
