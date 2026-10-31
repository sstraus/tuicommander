import commander from '../../src-tauri/src/themes/commander.json';
import paper from '../../src-tauri/src/themes/vscode-light.json';
import perimeter from '../../src/__tests__/fixtures/ego-perimeter/effective-default.json';
import {settingsCatalogue} from '../../src/generated/settingsCatalogue';
export const isTauri=()=>new URLSearchParams(location.search).get('browser')!=='1';
export const owningConnectionFor=()=>undefined;
export const buildHttpUrl=(p:string)=>'http://127.0.0.1:15391/blocked'+p;
export class HttpRpcError extends Error {}
export const listen=async()=>()=>{};
export const subscribeEvents=async()=>()=>{};
export const subscribePty=async()=>()=>{};
export const guardTauriUnlisten=(fn:any)=>fn;
export const emitLocalEvent=()=>{};
export const appLogger={info(){},warn(){},error(){},debug(){}};
export const calls:string[]=[];
const config={shell:null,font_family:'monospace',font_size:13,theme:'commander',mcp_server_enabled:true,disabled_native_tools:[],collapse_tools:false,services:{server:{enabled:false,port:9876,ipv6_enabled:false},auth:{username:'',password_hash:'',session_token_duration_secs:86400,lan_auth_bypass:false},relay:{enabled:false,url:'',session_id:''}}};
export async function invoke(cmd:string,args?:any):Promise<any>{calls.push(cmd);switch(cmd){
case 'list_themes': return [themeEntry('commander', commander), themeEntry('vscode-light', paper)];
case 'plugin_settings_action': return pluginAction(args);
case 'settings_action': {
 // Read-only synthetic UI state, derived from shipped descriptors; never service evidence.
 const request=args.request;
 if(request.action!=='get')return {error:{code:'unavailable',nextAction:'get',persisted:false}};
 const values=Object.fromEntries((request.paths??[]).map((path:string)=>{
  const field=Object.values(settingsCatalogue.fields).find(field=>field.store===request.store&&field.path===path);
  const value=field&&'default' in field?field.default:null;
  return [path,{stored:value,effective:value,source:'visual fixture'}];
 }));
 return {revision:'visual-fixture-only',values};
}
case 'list_discovered_ssh_hosts': return {hosts:[],hashed_count:0};
case 'list_ssh_agent_keys': return {keys:[],agent_type:'fixture'};
case 'list_remote_connections': case 'remote_connection_statuses': return [];
case 'load_config':return structuredClone(config);
case 'get_config_defaults':return {};
case 'get_home_directory':return '/workspace';
case 'get_cli_status':return {installed:true,path:'/fixture/tuic',version_match:true,auto_updatable:true};
case 'mdkb_status':return {available:false,connected:false};
case 'load_repo_settings':return {};
case 'load_mcp_upstreams':return {servers:[]};
case 'get_mcp_upstream_status':return {upstreams:[]};
case 'get_mcp_status':return {enabled:true,running:true,native_tools:[{name:'session',summary:'Manage terminal sessions',description:'Fixture tool'}],active_sessions:0,mcp_clients:0,max_sessions:32,remote_port:null,reachable:null};
case 'get_relay_status':return {enabled:false,connected:false,url:'',session_id:''};
case 'get_local_ips':return [];
case 'get_tailscale_status':return {installed:false,running:false,ip:null};
case 'get_remote_connections':case 'load_remote_connections':return {};
case 'get_dictation_config': return new URLSearchParams(location.search).has('voice-audit') ? {enabled:false,hotkey:'F5',language:'en',model:'small',speech_engine:new URLSearchParams(location.search).get('engine')??'pocket',speech_voice:'alba',speech_command:['speech-tool','{out}']} : undefined;
case 'get_model_info': return new URLSearchParams(location.search).has('voice-audit') ? [{name:'small',display_name:'Small',size_hint_mb:488,downloaded:true,actual_size_mb:488},{name:'large-v3-turbo',display_name:'Large V3 Turbo',size_hint_mb:1620,downloaded:false,actual_size_mb:0}] : [];
case 'get_speech_assets': return new URLSearchParams(location.search).has('voice-audit') ? [{id:'runtime',kind:'runtime',display_name:'Pocket TTS runtime',language:null,state:'ready',download_bytes:12000000,voices:[]},{id:'english',kind:'language',display_name:'English',language:'en',state:'ready',download_bytes:220000000,voices:['alba']},{id:'voice-alba',kind:'voice',display_name:'Alba',language:'en',voice:'alba',state:'ready',download_bytes:1000000,voices:[]}] : [];
case 'get_speech_voices': return new URLSearchParams(location.search).has('voice-audit') ? [{id:'alba',source:'catalogue'}] : [];
case 'list_audio_output_devices':case 'list_audio_devices':case 'get_edge_voices':case 'fetch_plugin_registry':case 'list_installed_mcp_integrations':case 'github_list_accounts':return [];
case 'get_agent_mcp_status':return {supported:true,installed:false,config_path:null,shared_settings_file:false};
case 'github_auth_status':return {authenticated:true,login:'fixture-account',avatar_url:null,source:'oauth',scopes:'repo'};
case 'github_diagnostics':return {circuit_breaker_open:false,circuit_breaker_status:'closed',repos_not_found:[],repos_monitored:0};
case 'github_resolve_repos':return {};
case 'circleci_token_status':return {configured:false,source:'none'};
case 'check_microphone_permission':return 'not_determined';
case 'telegram_settings':return {enabled:false,token_set:false,bot_alias:null,registered_agent_name:null,chats:[],pairing_pending:false,recent_events:[],connected:false,polling_owner:null,last_error:null,last_message_time:null};
case 'ego_providers':return {defaultModel:'fixture/example',providers:[{name:'fixture',credential:{state:'absent',detail:'Illustrative fixture only'},models:[{slug:'fixture/example',name:'Example model',available:false,unavailable:'No credential in isolated fixture'}]}]};
case 'ego_perimeter':return {profile:null,roots:{rootDir:'/workspace',rootAccess:'read-write',readAllowlist:'',writableDirs:''},networkEnabled:false,execEnforcement:'notChecked',effective:{...perimeter,roots:[{path:'/workspace',access:'read-write',source:'default'}]},preview:'Illustrative configuration; not a live enforcement check'};
case 'load_keybindings':return [];
case 'load_agents_config':return {agents:{},hibernate_idle_minutes:60,hibernate_scope:'background_only'};
default:return undefined;
}}
export const rpc=invoke;

// Explicit, isolated visual scenarios. These are synthetic UI fixtures, not live service evidence.
export const scenario = new URLSearchParams(location.search).get('state') ?? 'ready';
export const pluginField = {id:'plugins.fixture.endpoint',path:'/endpoint',label:'endpoint',schema:{type:'string',title:'Server endpoint'},operations:['get','set','reset'],sensitivity:'public',order:0,default:'https://example.test'};
export const pluginManifest = {id:'fixture',name:'Fixture integration',version:'1.0.0',minAppVersion:'1.0.0',description:'Isolated visual fixture',main:'main.js',capabilities:[],settings:{schema:{type:'object',additionalProperties:false,properties:{endpoint:{type:'string',title:'Server endpoint','x-setting':{sensitivity:'public',apply:'live',effect:'Endpoint changes',operations:['get','set','reset'],confirmationClass:'ordinary'}}}},defaults:{endpoint:'https://example.test'}}};
let revision = 0;
async function pluginAction(args:any) {
 if(args.action.action==='describe') return {fields:{[pluginField.id]:pluginField},collections:[],gaps:[],exclusions:[]};
 if(args.action.action==='get') {
  if(scenario==='loading') return new Promise(()=>{});
  if(scenario==='error') throw new Error('Isolated visual read failure');
  return {revision:`r${++revision}`,values:{'/endpoint':{effective:revision>1?'https://latest.example.test':'https://example.test',source:'stored'}}};
 }
 if(scenario==='stale') throw new Error('Isolated visual revision conflict');
 return {effects:[],restart:[]};
}

// Shipped palettes, projected to the Rust list_themes response, not service state.
function themeEntry(key: string, source: typeof commander) {
 const aliases: Record<string,string> = {background:'bg_primary',surface:'bg_secondary',surfaceElevated:'bg_tertiary',highlight:'bg_highlight',foreground:'fg_primary',foregroundSecondary:'fg_secondary',mutedForeground:'fg_muted',accentHover:'accent_hover',accentForeground:'text_on_accent',errorForeground:'text_on_error',successForeground:'text_on_success'};
 return {key,name:source.name,terminal:{background:source.background,foreground:source.foreground,cursor:source.cursorColor,cursor_accent:source.cursorAccent,selection_background:source.selectionBackground,ansi:[source.black,source.red,source.green,source.yellow,source.blue,source.purple,source.cyan,source.white,source.brightBlack,source.brightRed,source.brightGreen,source.brightYellow,source.brightBlue,source.brightPurple,source.brightCyan,source.brightWhite]},app_chrome:Object.fromEntries(Object.entries(source.appChrome).map(([name,value])=>[aliases[name]??name,value]))};
}
