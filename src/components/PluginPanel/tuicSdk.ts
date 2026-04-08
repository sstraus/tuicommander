/** Version of the TUIC SDK injected into plugin iframes */
export const TUIC_SDK_VERSION = "1.0";

/**
 * Self-contained <script> block injected into every plugin iframe.
 * Provides window.tuic API for host integration (open files, terminals)
 * and intercepts <a href="tuic://..."> link clicks.
 */
export const TUIC_SDK_SCRIPT = `<script id="tuic-sdk">
(function(){
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
    }
  };
  window.tuic=tuic;
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
        var path=decodeURIComponent(url.pathname);
        var pinned=a.hasAttribute("data-pinned");
        tuic.open(path,{pinned:pinned});
      }else if(cmd==="edit"){
        var epath=decodeURIComponent(url.pathname);
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
