/** Version of the TUIC SDK injected into plugin iframes */
export const TUIC_SDK_VERSION = "1.0";

/**
 * Self-contained <script> block injected into every plugin iframe.
 * Provides window.tuic API for host integration (open files, terminals)
 * and intercepts <a href="tuic://..."> link clicks.
 */
export const TUIC_SDK_SCRIPT = `<script id="tuic-sdk">
(function(){
  var _activeRepo=null;
  var _repoListeners=[];
  var _filePending={};
  var _reqId=0;
  var _msgListeners=[];
  var _theme=null;
  var _themeListeners=[];
  var tuic={
    version:"${TUIC_SDK_VERSION}",
    open:function(path,opts){
      parent.postMessage({type:"tuic:open",path:path,pinned:!!(opts&&opts.pinned)},"*");
    },
    edit:function(path,opts){
      parent.postMessage({type:"tuic:edit",path:path,line:(opts&&opts.line)||0},"*");
    },
    terminal:function(repoPath){
      parent.postMessage({type:"tuic:terminal",repoPath:repoPath},"*");
    },
    activeRepo:function(){return _activeRepo;},
    onRepoChange:function(cb){if(typeof cb==="function")_repoListeners.push(cb);},
    offRepoChange:function(cb){_repoListeners=_repoListeners.filter(function(f){return f!==cb;});},
    toast:function(title,opts){
      parent.postMessage({type:"tuic:toast",title:title,message:(opts&&opts.message)||"",level:(opts&&opts.level)||"info",sound:!!(opts&&opts.sound)},"*");
    },
    clipboard:function(text){
      parent.postMessage({type:"tuic:clipboard",text:text||""},"*");
    },
    getFile:function(path){
      return new Promise(function(resolve,reject){
        var id=++_reqId;
        _filePending[id]={resolve:resolve,reject:reject};
        parent.postMessage({type:"tuic:get-file",path:path,requestId:id},"*");
      });
    },
    onMessage:function(cb){if(typeof cb==="function")_msgListeners.push(cb);},
    offMessage:function(cb){_msgListeners=_msgListeners.filter(function(f){return f!==cb;});},
    send:function(data){
      parent.postMessage({type:"tuic:plugin-message",payload:data},"*");
    },
    get theme(){return _theme;},
    onThemeChange:function(cb){if(typeof cb==="function")_themeListeners.push(cb);},
    offThemeChange:function(cb){_themeListeners=_themeListeners.filter(function(f){return f!==cb;});}
  };
  window.tuic=tuic;
  window.addEventListener("message",function(e){
    if(!e.data||typeof e.data!=="object")return;
    var t=e.data.type;
    if(t==="tuic:repo-changed"){
      _activeRepo=e.data.repoPath||null;
      for(var i=0;i<_repoListeners.length;i++)try{_repoListeners[i](_activeRepo);}catch(err){}
    }else if(t==="tuic:get-file-result"){
      var p=_filePending[e.data.requestId];
      if(p){delete _filePending[e.data.requestId];if(e.data.error)p.reject(new Error(e.data.error));else p.resolve(e.data.content);}
    }else if(t==="tuic:host-message"){
      for(var j=0;j<_msgListeners.length;j++)try{_msgListeners[j](e.data.payload);}catch(err){}
    }else if(t==="tuic:theme-changed"){
      _theme=e.data.theme||null;
      for(var k=0;k<_themeListeners.length;k++)try{_themeListeners[k](_theme);}catch(err){}
    }
  });
  document.addEventListener("click",function(e){
    var a=e.target;
    while(a&&a.tagName!=="A")a=a.parentElement;
    if(!a||!a.href)return;
    var href=a.getAttribute("href");
    if(!href||href.indexOf("tuic://")!==0)return;
    e.preventDefault();
    try{
      var url=new URL(href);
      var cmd=url.hostname;
      if(cmd==="open"){
        var rawPath=decodeURIComponent(url.pathname);
        var path=rawPath.length>1&&rawPath.charAt(0)==="/"?rawPath.slice(1):rawPath;
        var pinned=a.hasAttribute("data-pinned");
        tuic.open(path,{pinned:pinned});
      }else if(cmd==="edit"){
        var rawEpath=decodeURIComponent(url.pathname);
        var epath=rawEpath.length>1&&rawEpath.charAt(0)==="/"?rawEpath.slice(1):rawEpath;
        var line=parseInt(url.searchParams.get("line")||"0",10);
        tuic.edit(epath,{line:line});
      }else if(cmd==="terminal"){
        var repo=url.searchParams.get("repo");
        if(repo)tuic.terminal(repo);
      }
    }catch(err){}
  },true);
  // Reload UX (story for Boss request, 2026-04-15): cross-origin iframes don't
  // bubble contextmenu/keydown to the parent, so forward them explicitly.
  // Parent opens its own context menu and reload action — from inside the
  // iframe this is the only way the app-level menu knows about a right-click
  // or Cmd/Ctrl+R that happened while the iframe was focused.
  document.addEventListener("contextmenu",function(e){
    e.preventDefault();
    parent.postMessage({type:"tuic:context-menu",x:e.clientX,y:e.clientY},"*");
  });
  document.addEventListener("keydown",function(e){
    if((e.metaKey||e.ctrlKey)&&!e.shiftKey&&!e.altKey&&(e.key==="r"||e.key==="R")){
      e.preventDefault();
      parent.postMessage({type:"tuic:reload-request"},"*");
    }
  });
  parent.postMessage({type:"tuic:sdk-request"},"*");
})();
</script>`;
