import { render } from 'solid-js/web';
import '../../global.css';

window.addEventListener('error',event=>{window.controlGeometryError=event.message;});
window.addEventListener('unhandledrejection',event=>{window.controlGeometryError=String(event.reason?.stack??event.reason);});

// Internal layout state only. No plugin code, package install or backend is executed.
window.fetch=async input=>{
 const path=new URL(typeof input==='string'?input:input.url,location.origin).pathname;
 if(path!=='/registry/plugins')throw new Error(`Unexpected geometry request: ${path}`);
 return new Response(JSON.stringify([{id:'fixture',name:'Fixture plugin',description:'A registry layout entry',latestVersion:'2.0.0',downloadUrl:'https://example.test/fixture.zip',capabilities:[]}]),{headers:{'content-type':'application/json'}});
};
const [{PluginsTab},{pluginStore},{registryStore},{dashboardRegistry}]=await Promise.all([
 import('../../components/SettingsPanel/tabs/PluginsTab'),import('../../stores/pluginStore'),import('../../stores/registryStore'),import('../../plugins/dashboardRegistry')
]);
pluginStore.registerPlugin('fixture',{loaded:true,manifest:{name:'Fixture plugin with a wrapping description',version:'1.0.0',description:'This long internal description verifies narrow row layout without loading a package.',capabilities:[]}});
pluginStore.getLogger('fixture').info('Fixture log entry');
dashboardRegistry.register({pluginId:'fixture',label:'Fixture dashboard',open:()=>{}});
await registryStore.fetch();
// Expose desktop-only controls for layout, with no native IPC or file operation.
window.__TAURI_INTERNALS__={invoke:async command=>{
 if(command==='get_plugin_readme_path')return '/fixture/README.md';
 throw new Error(`Unexpected geometry IPC: ${command}`);
}};
render(()=> <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)">
 <div id="installed"><PluginsTab settingsPluginIds={['fixture']} onOpenSettings={()=>{}} /></div>
 <div id="browse"><PluginsTab /></div>
</section>,document.getElementById('root'));
await new Promise(resolve=>requestAnimationFrame(resolve));
if(!document.querySelector('#installed button[aria-label="View plugin documentation"]'))throw new Error('Documentation icon did not mount');
document.querySelectorAll('#installed details').forEach(details=>details.open=true);
document.querySelector('#installed button[title="View logs"]').click();
document.querySelectorAll('#browse [role="tab"]')[1].click();
window.controlGeometryReady=true;
