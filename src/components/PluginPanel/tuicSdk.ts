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
      parent.postMessage({type:"tuic:toast",title:title,message:(opts&&opts.message)||"",level:(opts&&opts.level)||"info"},"*");
    },
    clipboard:function(text){
      parent.postMessage({type:"tuic:clipboard",text:text||""},"*");
    }
  };
  window.tuic=tuic;
  window.addEventListener("message",function(e){
    if(!e.data||typeof e.data!=="object")return;
    if(e.data.type==="tuic:repo-changed"){
      _activeRepo=e.data.repoPath||null;
      for(var i=0;i<_repoListeners.length;i++)try{_repoListeners[i](_activeRepo);}catch(err){}
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
})();
</script>`;
