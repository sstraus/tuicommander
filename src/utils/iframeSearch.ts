/**
 * Self-contained search overlay for iframes.
 *
 * Injected as a <script> block into same-origin iframes (PluginPanel srcdoc,
 * HtmlPreviewTab). Provides Cmd/Ctrl+F find-in-page with highlight, match
 * count, and keyboard navigation (Enter/Shift+Enter, Escape to close).
 *
 * Uses CSS custom properties from the TUIC theme when available, falls back
 * to sane defaults. Does not depend on the TUIC SDK — works standalone.
 */

export const IFRAME_SEARCH_SCRIPT = `<script id="tuic-search">
(function(){
  if(window.__tuicIframeSearch)return;
  window.__tuicIframeSearch=true;
  var overlay=null,input=null,countEl=null,matches=[],idx=-1,debounceId=0;

  function create(){
    if(overlay){overlay.style.display="flex";input.focus();input.select();return;}
    overlay=document.createElement("div");
    overlay.id="tuic-search-overlay";
    overlay.style.cssText="position:fixed;top:0;left:0;right:0;z-index:999999;display:flex;align-items:center;gap:6px;padding:5px 10px;background:var(--bg-secondary,#1e1e2e);border-bottom:1px solid var(--border-primary,#313244);font-family:system-ui,-apple-system,sans-serif;font-size:13px;color:var(--fg-primary,#cdd6f4);box-shadow:0 2px 8px rgba(0,0,0,.3);";

    input=document.createElement("input");
    input.type="text";
    input.placeholder="Find\\u2026";
    input.style.cssText="flex:1;min-width:120px;max-width:320px;padding:3px 8px;background:var(--bg-primary,#11111b);color:var(--fg-primary,#cdd6f4);border:1px solid var(--border-primary,#313244);border-radius:4px;outline:none;font-size:13px;";

    countEl=document.createElement("span");
    countEl.style.cssText="min-width:48px;text-align:center;color:var(--fg-secondary,#a6adc8);font-size:12px;white-space:nowrap;";
    countEl.textContent="0/0";

    var btnCss="padding:2px 6px;background:transparent;border:1px solid var(--border-primary,#313244);border-radius:3px;color:var(--fg-primary,#cdd6f4);cursor:pointer;font-size:12px;line-height:1;";
    var prev=document.createElement("button");prev.textContent="\\u25B2";prev.title="Previous (Shift+Enter)";prev.style.cssText=btnCss;
    var next=document.createElement("button");next.textContent="\\u25BC";next.title="Next (Enter)";next.style.cssText=btnCss;
    var close=document.createElement("button");close.textContent="\\u2715";close.title="Close (Escape)";close.style.cssText=btnCss;

    overlay.appendChild(input);
    overlay.appendChild(countEl);
    overlay.appendChild(prev);
    overlay.appendChild(next);
    overlay.appendChild(close);
    document.body.appendChild(overlay);

    input.addEventListener("input",function(){
      clearTimeout(debounceId);
      debounceId=setTimeout(function(){doSearch(input.value);},120);
    });
    input.addEventListener("keydown",function(e){
      if(e.key==="Enter"){e.preventDefault();e.shiftKey?goPrev():goNext();}
      if(e.key==="Escape"){e.preventDefault();closeSearch();}
      if((e.metaKey||e.ctrlKey)&&(e.key==="g"||e.key==="G")){e.preventDefault();e.shiftKey?goPrev():goNext();}
      e.stopPropagation();
    });
    prev.addEventListener("click",goPrev);
    next.addEventListener("click",goNext);
    close.addEventListener("click",closeSearch);
    input.focus();
  }

  function closeSearch(){
    if(overlay)overlay.style.display="none";
    clearMarks();
  }

  function clearMarks(){
    var marks=document.querySelectorAll("mark.tuic-sf");
    for(var i=0;i<marks.length;i++){
      var m=marks[i],p=m.parentNode;
      while(m.firstChild)p.insertBefore(m.firstChild,m);
      p.removeChild(m);
      p.normalize();
    }
    matches=[];idx=-1;
    if(countEl)countEl.textContent="0/0";
  }

  function doSearch(q){
    clearMarks();
    if(!q){return;}
    var walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT,null);
    var nodes=[];var n;
    while(n=walker.nextNode()){
      if(n.parentElement&&(n.parentElement.id==="tuic-search-overlay"||n.parentElement.closest("#tuic-search-overlay")))continue;
      nodes.push(n);
    }
    var lq=q.toLowerCase();
    for(var i=0;i<nodes.length;i++){
      var tn=nodes[i];
      var text=tn.textContent;
      var lt=text.toLowerCase();
      var pos=0;
      while((pos=lt.indexOf(lq,pos))!==-1){
        var before=tn.splitText(pos);
        var after=before.splitText(q.length);
        var mark=document.createElement("mark");
        mark.className="tuic-sf";
        mark.style.cssText="background:var(--warning,#f9e2af);color:var(--bg-primary,#1b1b2b);border-radius:2px;padding:0 1px;";
        before.parentNode.insertBefore(mark,before);
        mark.appendChild(before);
        matches.push(mark);
        tn=after;
        text=tn.textContent;
        lt=text.toLowerCase();
        pos=0;
      }
    }
    if(matches.length>0){idx=0;hilite();}
    updateCount();
  }

  function hilite(){
    for(var i=0;i<matches.length;i++){
      var cur=i===idx;
      matches[i].style.background=cur?"var(--accent-primary,#f38ba8)":"var(--warning,#f9e2af)";
      matches[i].style.color="var(--bg-primary,#1b1b2b)";
    }
    if(matches[idx])matches[idx].scrollIntoView({block:"center",behavior:"smooth"});
  }

  function updateCount(){
    if(countEl)countEl.textContent=matches.length>0?((idx+1)+"/"+matches.length):"0/0";
  }

  function goNext(){
    if(!matches.length)return;
    idx=(idx+1)%matches.length;
    hilite();updateCount();
  }

  function goPrev(){
    if(!matches.length)return;
    idx=(idx-1+matches.length)%matches.length;
    hilite();updateCount();
  }

  document.addEventListener("click",function(e){
    var a=e.target;
    while(a&&a.tagName!=="A")a=a.parentElement;
    if(!a||!a.href)return;
    var href=a.getAttribute("href")||"";
    if(href.charAt(0)==="#"){
      e.preventDefault();
      var el=document.getElementById(href.slice(1));
      if(el)el.scrollIntoView({behavior:"smooth"});
    }
  },true);

  document.addEventListener("keydown",function(e){
    if((e.metaKey||e.ctrlKey)&&!e.shiftKey&&!e.altKey&&(e.key==="f"||e.key==="F")){
      e.preventDefault();
      e.stopPropagation();
      create();
    }
    if((e.metaKey||e.ctrlKey)&&!e.shiftKey&&!e.altKey&&(e.key==="r"||e.key==="R")){
      e.preventDefault();
      e.stopPropagation();
      parent.postMessage({type:"tuic:reload-request"},"*");
    }
    if(e.key==="Escape"&&overlay&&overlay.style.display!=="none"){
      e.preventDefault();
      e.stopPropagation();
      closeSearch();
    }
  });
})();
</script>`;

/**
 * Scrollbar styling injected into same-origin iframes (HtmlPreviewTab srcdoc) so
 * their scrollbars match the rest of the app. Mirrors the global `::-webkit-scrollbar`
 * rule in global.css — the iframe is a separate document and can't inherit it.
 * Literal colors (no CSS vars) since the iframe has no access to the TUIC theme.
 */
export const IFRAME_SCROLLBAR_STYLE = `<style id="tuic-scrollbar">
*::-webkit-scrollbar{width:14px;height:14px}
*::-webkit-scrollbar-track{background:transparent}
*::-webkit-scrollbar-thumb{background:#37373d;border-radius:5px;border:2px solid transparent;background-clip:padding-box}
*::-webkit-scrollbar-thumb:hover{background:rgba(204,204,204,0.3);background-clip:padding-box}
*::-webkit-scrollbar-corner{background:transparent}
</style>`;
