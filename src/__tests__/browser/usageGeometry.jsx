import { render } from 'solid-js/web';
import { ClaudeUsageDashboard } from '../../components/ClaudeUsageDashboard/ClaudeUsageDashboard';
import '../../global.css';
window.__TAURI_INTERNALS__={invoke:async command=>{
 if(command==='get_claude_usage_api')throw new Error('Usage unavailable in layout fixture');
 if(command==='get_claude_session_stats')return {total_sessions:0,total_assistant_messages:0,total_user_messages:0,total_input_tokens:0,total_output_tokens:0,total_cache_creation_tokens:0,total_cache_read_tokens:0,model_usage:{},daily_activity:{},per_project:{},per_project_daily:{},active_hours:0};
 return [];
}};
render(()=><section id="geometry"><ClaudeUsageDashboard/></section>,document.getElementById('root'));
const ready=()=>{
 if(![...document.querySelectorAll('button')].some(button=>button.textContent.trim()==='Retry')){requestAnimationFrame(ready);return;}
 window.controlGeometryReady=true;
};
requestAnimationFrame(ready);
