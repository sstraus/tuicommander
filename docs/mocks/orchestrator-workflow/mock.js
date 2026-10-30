// Static design mock: no workflow API, provider, process or repository effects.
const live = document.body.dataset.view === "run";
const W = 1340;
const H = 690;
const esc = value => String(value).replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const clone = value => JSON.parse(JSON.stringify(value));
const author = (role, model, prompt) => ({ role, model, prompt, tools: ["MDKB", "Repository read", "Scoped write", "Report"] });
const node = (id, type, title, x, y, value, state = "waiting", cost = 0, extra = {}) => ({id,type,title,x,y,w:156,h:82,value,state,cost,...extra});
const initial = {
  portfolio: {
    nodes: [
      node("start","Start","Portfolio",32,280,"Run 104","done",0,{w:126}),
      node("fork-stories","Fork","Eligible stories",218,280,"2 branches","done",0,{w:142}),
      node("call-a","Call","Story A",424,118,"delivery@3","pending",2.40,{ref:3,branch:"A"}),
      node("call-b","Call","Story B",424,432,"delivery@3","running",2.34,{ref:3,branch:"B"}),
      node("ask-a","Ask","Scope · story A",650,52,"Pending · q1","pending",0.08),
      node("join-stories","Join all","Delivery receipts",822,280,"0 / 2 receipts","waiting",0),
      node("cohort","Gate","Proof cohort",1030,280,"2 obligations","waiting",0),
      node("batch","Run command","Batch tests",1030,492,"rb Rust · Mac JS","waiting",0),
      node("end","End","Complete",1220,492,"0 / 2 current","waiting",0,{w:100}),
      node("hold","Pause","Reconcile",688,552,"Uncertain effect","waiting",0,{w:132})
    ],
    edges: [
      {from:"start",to:"fork-stories",state:"done"},
      {from:"fork-stories",to:"call-a",label:"story A",state:"done"},
      {from:"fork-stories",to:"call-b",label:"story B",state:"running"},
      {from:"call-a",to:"join-stories",label:"receipt A"},
      {from:"call-b",to:"join-stories",label:"receipt B"},
      {from:"call-a",to:"ask-a",style:"pending",label:"needs input",fromSide:"top",toSide:"left"},
      {from:"ask-a",to:"call-a",style:"pending",label:"answer",fromSide:"bottom",toSide:"right",bend:220},
      {from:"join-stories",to:"cohort",label:"all"},
      {from:"cohort",to:"batch",label:"pass",fromSide:"bottom",toSide:"top"},
      {from:"batch",to:"end",label:"pass"},
      {from:"call-b",to:"hold",style:"reject",label:"failed / uncertain",fromSide:"bottom",toSide:"left"},
      {from:"cohort",to:"hold",style:"reject",label:"fail",fromSide:"bottom",toSide:"right",bend:490}
    ]
  },
  delivery: {
    nodes: [
      node("implement","Author","Implement",36,274,"Scoped worktree","done",1.18,{...author("Implementer","GPT-6.1 Sol","Implement {{story.criteria}} in {{worktree}}. Report {{artifact.digest}}.")}),
      node("fork-checks","Fork","Review + checks",244,274,"2 branches","done",0,{w:142}),
      node("critic","Author","Critic",446,114,"Review 2 / 2","running",1.06,{...author("Independent critic","Claude Opus 5.5","Review {{artifact.digest}} against {{story.criteria}}. Report FIX / STORY / ACCEPT."),limit:2}),
      node("checks","Run command","Pre-land checks",446,432,"Passed · 86 tests","done",0,{argv:"cargo nextest run -p tuic-terminal",target:"rb · Linux aarch64"}),
      node("judge","Judge","Review policy",660,114,"Max 2 reviews","waiting",0.10,{limit:2}),
      node("join-checks","Join all","Review receipts",876,274,"1 / 2 receipts","waiting",0),
      node("integrate","Integrate","Land",1090,274,"Canonical lock","waiting",0),
      node("proof","Gate","Proof gate",1090,458,"Mutation · CRAP","waiting",0),
      node("tests","Run command","Batch tests",868,558,"Post-landing","waiting",0,{argv:"pnpm vitest run",target:"Mac · JS; rb · Rust"}),
      node("delivered","End","Delivered",658,558,"Proof current","waiting",0,{w:136}),
      node("ask","Ask","Review decision",660,352,"Branch B","waiting",0),
      node("pause","Pause","Reconcile",1112,68,"Merge receipt","waiting",0,{w:132})
    ],
    edges: [
      {from:"implement",to:"fork-checks",state:"done"},
      {from:"fork-checks",to:"critic",state:"running",label:"review"},
      {from:"fork-checks",to:"checks",state:"done",label:"checks"},
      {from:"critic",to:"judge",label:"findings"},
      {from:"judge",to:"join-checks",label:"yes"},
      {from:"checks",to:"join-checks",label:"receipt",state:"done"},
      {from:"judge",to:"implement",style:"reject",label:"no · repair 1 / 1",fromSide:"top",toSide:"top",over:38},
      {from:"judge",to:"ask",style:"pending",label:"uncertain",fromSide:"bottom",toSide:"top"},
      {from:"ask",to:"judge",style:"pending",label:"answer",fromSide:"left",toSide:"left",via:624},
      {from:"join-checks",to:"integrate",label:"all"},
      {from:"integrate",to:"proof",label:"merged",fromSide:"bottom",toSide:"top"},
      {from:"integrate",to:"pause",style:"reject",label:"uncertain",fromSide:"top",toSide:"bottom"},
      {from:"proof",to:"tests",label:"pass",fromSide:"bottom",toSide:"right"},
      {from:"tests",to:"delivered",label:"pass",fromSide:"left",toSide:"right"},
      {from:"proof",to:"pause",style:"reject",label:"fail",fromSide:"right",toSide:"right",via:1300}
    ]
  }
};
let graphs = clone(initial);
let publishedGraphs = clone(initial);
const revisions = {portfolio:{draft:4,published:3,dirty:true},delivery:{draft:4,published:3,dirty:true}};
let graphName = "portfolio";
let selected = live ? "ask-a" : "call-b";
let branch = "B";
let answered = false;
let drag = null;
let moved = false;
let added = 0;
const events = live ? [
  ["130","23:14:08","Join all","Waiting · 0/2"],
  ["129","23:14:06","B / Critic","Running · 2/2"],
  ["128","23:14:04","A / Ask","Pending · q1"]
] : [["04","Draft 4","delivery","Review cap · 2"],["03","Published 3","portfolio","delivery@3"],["02","Published 3","policy","Repairs · 1"]];
const icon = '<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M5 2h2v2H5V2zm4 0h2v2H9V2zM5 7h2v2H5V7zm4 0h2v2H9V7zM5 12h2v2H5v-2zm4 0h2v2H9v-2z"/></svg>';
const families = {Author:["Author"],Build:["Run command","Integrate"],Check:["Judge","Gate"],Route:["Fork","Join all","Call","Loop"],Hold:["Ask","Pause","Timer"]};
function markDirty() {
  const r = revisions[graphName];
  if (r.draft === r.published) r.draft++;
  r.dirty = true;
  versions();
}
function versions() {
  const r = revisions[graphName];
  document.getElementById("version").textContent = live ? "Run 104 · pinned v3" : `Draft ${r.draft} · published v${r.published}`;
  document.getElementById("graph-title").textContent = graphName === "portfolio" ? "Portfolio delivery" : `Story ${branch} / delivery`;
  if (!live) {
    document.getElementById("publish").textContent = `Publish v${r.draft}`;
    document.getElementById("publish").disabled = !r.dirty;
    document.getElementById("draft-indicator").textContent = "";
  }
  document.querySelectorAll("[data-graph]").forEach(el => el.setAttribute("aria-pressed",String(el.dataset.graph === graphName)));
}
function renderShell() {
  document.getElementById("app").innerHTML = `<header><a class="brand" href="index.html">TUICommander</a><div class="header-title"><h1 id="graph-title">Portfolio delivery</h1><span class="version" id="version"></span></div><span class="header-spacer"></span><span class="counter">42.6k <small>tokens</small></span><span class="counter">$4.82 <small>est.</small></span><a class="nav-link ${!live?"current":""}" href="option-b-editor.html">Editor</a><a class="nav-link ${live?"current":""}" href="option-b-run.html">Live run</a>${live?'<span class="badge accent" id="run-state">Running · 1 Ask</span>':'<button id="discard">Discard</button><button class="primary" id="publish">Publish v4</button>'}</header>
  <div class="workspace"><aside class="library" id="library"><div><h3>Definition</h3><div class="definition"><strong>Portfolio delivery</strong><small>Option B · v3</small></div></div><section id="palette"><h3>Nodes</h3>${Object.entries(families).map(([family,items])=>`<div class="palette-group"><h3>${family}</h3><div class="palette-items">${items.map(type=>`<button class="palette-item" data-add="${type}" draggable="${!live}" title="Drag ${type} onto the canvas, or click to add" ${live?"disabled":""}>${icon}${type}</button>`).join("")}</div></div>`).join("")}</section><section class="runqueue"><h3>Run queue · 3</h3><div class="queue-row"><span class="dot active"></span><strong>104 · TUIC</strong><small>Running · v3</small></div><div class="queue-row"><span class="dot hold"></span><strong>105 · ego</strong><small>Resource wait</small></div><div class="queue-row"><span class="dot"></span><strong>106 · mdkb</strong><small>Queued</small></div></section></aside>
  <main><div class="canvas-toolbar"><div class="breadcrumbs"><button data-graph="portfolio" id="portfolio-tab">Portfolio</button><span class="muted">/</span><button data-graph="delivery" id="delivery-tab">delivery@3 · B</button></div><button class="mobile-only" id="palette-toggle">Palette / queue</button><span class="toolbar-meta" id="draft-indicator">${live?"Cursor 130":""}</span></div><div class="viewport" id="viewport"><svg class="graph-svg" id="graph" viewBox="0 0 ${W} ${H}" aria-label="Workflow node canvas" role="group"></svg><div class="graph-status"><span class="legend"><span class="legend-line"></span>success</span><span class="legend"><span class="legend-line reject"></span>fail / reject</span><span id="graph-count"></span></div></div><section class="event-panel"><div class="event-heading"><strong>${live?"Events":"Draft changes"}</strong><span class="muted" id="event-cursor">${live?"#130":"v4"}</span></div><div id="events"></div></section></main>
  <aside class="inspector" id="inspector" aria-label="Node inspector"></aside></div><footer><span>Option B · canvas mock · illustrative usage</span><span id="status" role="status" aria-live="polite"></span><span id="pin">Run 104 · pinned v3</span></footer>`;
  document.getElementById("portfolio-tab").onclick=()=>openGraph("portfolio");
  document.getElementById("delivery-tab").onclick=()=>openGraph("delivery");
  document.getElementById("palette-toggle").onclick=()=>document.getElementById("library").classList.toggle("open");
  document.querySelectorAll("[data-add]").forEach(el=>{
    el.onclick=()=>addNode(el.dataset.add);
    el.ondragstart=event=>{event.dataTransfer.setData("application/x-workflow-node",el.dataset.add);event.dataTransfer.effectAllowed="copy";};
  });
  const canvas=document.getElementById("graph");
  canvas.ondragover=event=>{if(!live&&event.dataTransfer.types.includes("application/x-workflow-node")){event.preventDefault();event.dataTransfer.dropEffect="copy";}};
  canvas.ondrop=event=>{
    const type=event.dataTransfer.getData("application/x-workflow-node");
    if(live||!Object.values(families).flat().includes(type))return;
    event.preventDefault();addNode(type,point(event));
  };
  if(!live){
    document.getElementById("publish").onclick=publish;
    document.getElementById("discard").onclick=()=>{
      graphs[graphName]=clone(publishedGraphs[graphName]);revisions[graphName].dirty=false;
      selected=graphName==="portfolio"?"call-b":"critic";renderGraph();renderInspector();versions();status("Draft discarded");
    };
  }
  versions();renderGraph();renderInspector();renderEvents();
}
function status(value){document.getElementById("status").textContent=value;}
function renderEvents(){document.getElementById("events").innerHTML=events.slice(0,8).map(e=>`<div class="event"><span class="muted">#${esc(e[0])}</span><time class="muted">${esc(e[1])}</time><span>${esc(e[2])}</span><span>${esc(e[3])}</span></div>`).join("");}
function openGraph(name,story="B"){
  graphName=name;branch=story;selected=name==="portfolio"?(live?"ask-a":"call-b"):"critic";
  document.getElementById("delivery-tab").textContent=`delivery@3 · ${story}`;
  document.getElementById("viewport").scrollTo(0,0);
  versions();renderGraph();renderInspector();
}
function graph(){return graphs[graphName];}
function subject(){return graph().nodes.find(n=>n.id===selected);}
function anchor(n,side){return side==="top"?[n.x+n.w/2,n.y]:side==="bottom"?[n.x+n.w/2,n.y+n.h]:side==="left"?[n.x,n.y+n.h/2]:[n.x+n.w,n.y+n.h/2];}
function geometry(edge){
  const a=graph().nodes.find(n=>n.id===edge.from),b=graph().nodes.find(n=>n.id===edge.to);
  const p=anchor(a,edge.fromSide||"right"),q=anchor(b,edge.toSide||"left");
  let d,x,y;
  if(edge.over!==undefined){d=`M${p} V${edge.over} H${q[0]} V${q[1]}`;x=(p[0]+q[0])/2;y=edge.over-8;}
  else if(edge.via!==undefined){d=`M${p} H${edge.via} V${q[1]} H${q[0]}`;x=edge.via;y=(p[1]+q[1])/2;}
  else if(edge.bend!==undefined){d=`M${p} V${edge.bend} H${q[0]} V${q[1]}`;x=(p[0]+q[0])/2;y=edge.bend-8;}
  else if((edge.fromSide==="bottom"||edge.fromSide==="top")&&(edge.toSide==="top"||edge.toSide==="bottom")){const cy=(p[1]+q[1])/2;d=`M${p} C${p[0]},${cy} ${q[0]},${cy} ${q}`;x=(p[0]+q[0])/2+28;y=cy;}
  else{const cx=(p[0]+q[0])/2;d=`M${p} C${cx},${p[1]} ${cx},${q[1]} ${q}`;x=cx;y=(p[1]+q[1])/2-10;}
  return{d,x,y};
}
function nodeState(n){
  if(graphName!=="delivery"||branch!=="A")return n.state;
  if(["implement","fork-checks","critic","checks"].includes(n.id))return "done";
  if(n.id==="ask")return answered?"done":"pending";
  if(n.id==="judge")return answered?"running":"pending";
  return "waiting";
}
function nodeValue(n){
  if(live&&graphName==="delivery"&&branch==="A"){
    if(n.id==="critic")return "Reviewed · 2/2";
    if(n.id==="judge")return answered?"Ready · answer":"Awaiting input";
    if(n.id==="ask")return answered?"Answered · A":"Pending · A";
  }
  if(!live){if(n.id==="critic")return `${n.limit} total reviews`;if(n.id==="judge")return `Max ${n.limit} reviews`;if(n.type==="Call")return `delivery@${n.ref}`;return n.value.replace("Pending · q1","Branch-local input").replace("Review 2 / 2","2 total reviews");}
  if(n.id==="call-a")return answered?"Ready · A":"Ask pending · A";
  if(n.id==="call-b")return "Critic running · 2/2";
  if(n.id==="ask-a")return answered?"Answered · q1":"Pending · q1";
  return n.value;
}
function renderGraph(){
  const svg=document.getElementById("graph");
  svg.innerHTML=`<defs>${["normal","done","running","reject","pending"].map((s,i)=>`<marker id="arrow-${s}" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0 0 L8 4 L0 8 Z" fill="${["var(--fg-muted)","var(--success)","var(--accent)","var(--error)","var(--attention)"][i]}"/></marker>`).join("")}</defs><rect class="region" x="${graphName==="portfolio"?390:416}" y="${graphName==="portfolio"?98:90}" width="${graphName==="portfolio"?222:628}" height="${graphName==="portfolio"?438:456}"/><text class="region-label" x="${graphName==="portfolio"?406:432}" y="${graphName==="portfolio"?86:78}">${graphName==="portfolio"?"FORK · ELIGIBLE STORIES":"FORK · INDEPENDENT REVIEW + CHECKS"}</text><g id="edge-layer">${graph().edges.map((e,i)=>{const g=geometry(e),style=e.style||(live?e.state:"")||"normal";return `<g data-edge="${i}"><path class="edge ${style}" d="${g.d}" marker-end="url(#arrow-${style})"/>${e.label?`<g class="edge-label ${e.style||""}" transform="translate(${g.x},${g.y})"><rect x="${-e.label.length*3.4-5}" y="-10" width="${e.label.length*6.8+10}" height="16" rx="3"/><text y="1">${esc(e.label)}</text></g>`:""}</g>`;}).join("")}</g><g id="node-layer">${graph().nodes.map(n=>`<g class="node ${live?nodeState(n):""} ${selected===n.id?"selected":""}" data-node="${n.id}" data-state="${live?nodeState(n):"draft"}" tabindex="0" role="button" aria-label="${esc(n.type+" · "+n.title)}" aria-pressed="${selected===n.id}" transform="translate(${n.x},${n.y})"><rect class="box" width="${n.w}" height="${n.h}"/><rect class="stripe" x="0" y="12" width="3" height="${n.h-24}"/><text class="type" x="12" y="20">${esc(n.type.toUpperCase())}</text><text class="title" x="12" y="41">${esc(n.title)}</text><text class="value" x="12" y="63">${esc(nodeValue(n))}</text>${live?`<text class="cost" x="${n.w-10}" y="20">$${n.cost.toFixed(2)}</text>`:""}<circle class="port" cx="0" cy="${n.h/2}" r="3"/><circle class="port" cx="${n.w}" cy="${n.h/2}" r="3"/></g>`).join("")}</g>`;
  document.getElementById("graph-count").textContent=`${graph().nodes.length} nodes · ${graph().edges.length} edges`;
  svg.querySelectorAll("[data-node]").forEach(el=>{
    el.addEventListener("pointerdown",event=>{
      if(event.button!==0)return;
      selected=el.dataset.node;svg.querySelectorAll("[data-node]").forEach(n=>n.classList.toggle("selected",n===el));renderInspector();
      if(live)return;
      const p=point(event),n=subject();drag={id:n.id,dx:p.x-n.x,dy:p.y-n.y,startX:n.x,startY:n.y,pointer:event.pointerId};moved=false;svg.setPointerCapture(event.pointerId);event.preventDefault();
    });
    el.addEventListener("click",()=>{if(!moved){selected=el.dataset.node;renderInspector();svg.querySelectorAll("[data-node]").forEach(n=>{n.classList.toggle("selected",n===el);n.setAttribute("aria-pressed",String(n===el));});}});
    el.addEventListener("dblclick",()=>{const n=graph().nodes.find(n=>n.id===el.dataset.node);if(n.type==="Call")openGraph("delivery",n.branch);});
    el.addEventListener("keydown",e=>{
      if(e.key==="Enter"){selected=el.dataset.node;renderInspector();}
      if(!live&&["ArrowLeft","ArrowRight","ArrowUp","ArrowDown"].includes(e.key)){e.preventDefault();selected=el.dataset.node;const n=subject(),step=e.shiftKey?20:5;n.x+=e.key==="ArrowLeft"?-step:e.key==="ArrowRight"?step:0;n.y+=e.key==="ArrowUp"?-step:e.key==="ArrowDown"?step:0;markDirty();renderGraph();document.querySelector(`[data-node="${n.id}"]`).focus();}
    });
  });
  svg.onpointermove=e=>{
    if(!drag)return;const p=point(e),n=subject();n.x=Math.max(12,Math.min(W-n.w-12,p.x-drag.dx));n.y=Math.max(12,Math.min(H-n.h-12,p.y-drag.dy));moved ||= Math.abs(n.x-drag.startX)+Math.abs(n.y-drag.startY)>3;
    const el=svg.querySelector(`[data-node="${n.id}"]`);el.setAttribute("transform",`translate(${n.x},${n.y})`);
    graph().edges.forEach((edge,i)=>{const g=geometry(edge),group=svg.querySelector(`[data-edge="${i}"]`);group.querySelector("path").setAttribute("d",g.d);group.querySelector(".edge-label")?.setAttribute("transform",`translate(${g.x},${g.y})`);});
  };
  svg.onpointerup=e=>{if(drag){if(moved){markDirty();status(`${subject().title} · ${Math.round(subject().x)}, ${Math.round(subject().y)}`);}svg.releasePointerCapture(e.pointerId);drag=null;}};
  svg.onpointercancel=()=>{drag=null;};
}
function point(e){const p=new DOMPoint(e.clientX,e.clientY);return p.matrixTransform(document.getElementById("graph").getScreenCTM().inverse());}
function property(label,value){return `<div class="property"><span>${label}</span><span>${esc(value)}</span></div>`;}
function renderInspector(){
  const n=subject();if(!n)return;
  let content=`<div class="inspector-top"><h3>Inspector</h3><span class="badge">${esc(n.type)}</span></div><h2>${esc(n.title)}</h2><small class="mono">${esc(n.id)}</small>`;
  if(live){
    content+=`<div class="node-cost">$${n.cost.toFixed(2)} <small>estimated</small></div>${property("State",nodeValue(n))}${property("Subject","tree:7af…")}${property("Definition","v3")}${property("Executor",n.id==="batch"?"Mac / rb":"Owning daemon")}${property("Receipt",n.state==="done"?"Current":"Pending")}`;
    if(n.type==="Call")content+='<button class="full" id="open-call">Open delivery</button>';
    content+=`<section class="section"><h3>Pending input · A</h3><span id="decision-status" class="badge ${answered?"success":"attention"}">${answered?"Answered · q1":"Pending · q1"}</span><div class="decision-title">Worktree scope</div>${property("Current","Original scope")}${property("Recommendation","Keep scope")}<div class="answer-options"><button class="primary" id="answer" ${answered?"disabled":""}>Keep scope</button><button id="expand-answer" ${answered?"disabled":""}>Expand scope</button></div></section><section class="section"><h3>Sibling · B</h3>${property("Critic","Running · 2/2")}${property("Checks","Passed · 86")}${property("Join all","1/2 receipts")}</section>`;
  }else{
    if(n.type==="Author")content+=`<label>Role<select id="role"><option>Implementer</option><option>Independent critic</option><option>Researcher</option><option>Planner</option></select></label><label>Model<select id="model"><option>GPT-6.1 Sol</option><option>Claude Opus 5.5</option><option>Claude Sonnet 5.5</option></select></label><label>Prompt<textarea id="prompt">${esc(n.prompt)}</textarea></label><fieldset><legend>Tools</legend>${["MDKB","Repository read","Scoped write","Report"].map((tool,i)=>`<label class="tool"><input type="checkbox" data-tool="${tool}" id="tool-${i}" ${n.tools.includes(tool)?"checked":""}>${tool}</label>`).join("")}</fieldset>`;
    if(n.limit!==undefined)content+=`<label>Total reviews<input id="limit" type="number" min="1" max="10" value="${n.limit}"></label>${property("Repairs",Math.max(0,n.limit-1))}${property("Exhausted","Ask")}`;
    if(n.type==="Call")content+=`<label>Definition<select id="call-version"><option value="3">delivery@3</option><option value="4">delivery@4</option></select></label>${property("Story",n.branch)}<button class="primary full" id="open-call">Open delivery</button>`;
    if(n.type==="Run command")content+=`<label>argv<input id="argv" value="${esc(n.argv||"batch-tests --pinned")}"></label><label>Target<select id="target"><option>Mac · JS; rb · Rust</option><option>rb · Linux aarch64</option><option>Mac · native</option></select></label>${property("Timeout","600s")}${property("Receipt","Pinned tree")}`;
    if(n.type==="Integrate")content+=property("Writer","Canonical · 1")+property("Base","Expected SHA")+property("Preflight","Checks + review")+property("Post-merge","Proof obligation");
    if(n.type==="Join all")content+=property("Branches","Exact fork set")+property("Required",graphName==="portfolio"?"2 deliveries":"Review + checks")+property("Late receipt","Generation fenced");
    if(n.type==="Gate")content+=property("Policy","Current evidence")+property("Checks","Mutation / CRAP")+property("Failure","Reconcile");
    if(n.type==="Ask")content+=property("Scope","Branch local")+property("Answer","Revision fenced")+property("Timeout","No consent");
    content+=`<section class="section"><h3>Edges</h3>${graph().edges.filter(e=>e.from===n.id).map(e=>property(e.label||"success",graph().nodes.find(t=>t.id===e.to).title)).join("")}<label>Connect to<select id="connect-target"><option value="">—</option>${graph().nodes.filter(t=>t.id!==n.id).map(t=>`<option value="${t.id}">${esc(t.title)}</option>`).join("")}</select></label><button class="full" id="connect">Connect · success</button></section>`;
    if(n.id.startsWith("new-"))content+='<button class="full" id="remove">Remove node</button>';
  }
  document.getElementById("inspector").innerHTML=content;
  const open=document.getElementById("open-call");if(open)open.onclick=()=>openGraph("delivery",n.branch||"B");
  if(live){document.getElementById("answer").onclick=()=>answer("Keep scope");document.getElementById("expand-answer").onclick=()=>answer("Expand scope");return;}
  for(const field of ["role","model","prompt","argv","target"]){const el=document.getElementById(field);if(!el)continue;if(n[field]!==undefined)el.value=n[field];el[el.tagName==="SELECT"?"onchange":"oninput"]=()=>{n[field]=el.value;markDirty();status(`${n.title} · ${field}`);};}
  document.querySelectorAll("[data-tool]").forEach(el=>el.onchange=()=>{n.tools=[...document.querySelectorAll("[data-tool]:checked")].map(t=>t.dataset.tool);markDirty();status(`${n.title} · ${n.tools.length} tools`);});
  const limit=document.getElementById("limit");if(limit)limit.oninput=()=>{const value=Number(limit.value);for(const target of graphs.delivery.nodes.filter(t=>t.limit!==undefined))target.limit=value;markDirty();renderGraph();status(`Total reviews · ${value} | repairs · ${Math.max(0,value-1)}`);};
  const version=document.getElementById("call-version");if(version){version.value=n.ref;version.onchange=()=>{n.ref=Number(version.value);markDirty();renderGraph();};}
  document.getElementById("connect").onclick=()=>{const target=document.getElementById("connect-target").value;if(target&&!graph().edges.some(e=>e.from===n.id&&e.to===target)){graph().edges.push({from:n.id,to:target});markDirty();renderGraph();renderInspector();}};
  const remove=document.getElementById("remove");if(remove)remove.onclick=()=>{graph().nodes=graph().nodes.filter(t=>t.id!==n.id);graph().edges=graph().edges.filter(e=>e.from!==n.id&&e.to!==n.id);selected=graph().nodes[0].id;markDirty();renderGraph();renderInspector();};
}
function answer(value){
  if(answered)return;answered=true;const a=graphs.portfolio.nodes.find(n=>n.id==="ask-a"),call=graphs.portfolio.nodes.find(n=>n.id==="call-a");a.state="done";call.state="running";
  events.unshift(["131","23:14:24","A / Ask",value]);document.getElementById("event-cursor").textContent="#131";document.getElementById("run-state").textContent="Running · 0 Ask";renderGraph();renderInspector();renderEvents();status("A · ready | B · critic running");
}
function addNode(type,position){
  const id=`new-${++added}`,extra=type==="Author"?author("Researcher","GPT-6.1 Sol","Research {{story.problem}}. Report evidence."):{};
  const x=position?Math.max(12,Math.min(W-168,position.x)):190+added*25;
  const y=position?Math.max(12,Math.min(H-94,position.y)):540;
  graph().nodes.push(node(id,type,type,x,y,"Unconnected","waiting",0,extra));selected=id;markDirty();renderGraph();renderInspector();status(`${type} · added`);
}
function publish(){
  const invalid=graph().nodes.some(n=>n.limit!==undefined&&(!Number.isInteger(n.limit)||n.limit<1||n.limit>10));
  const disconnected=graph().nodes.some(n=>n.id.startsWith("new-")&&!graph().edges.some(e=>e.from===n.id||e.to===n.id));
  if(invalid||disconnected){status(invalid?"Invalid review limit":"Unconnected node");return;}
  const r=revisions[graphName];r.published=r.draft;r.dirty=false;publishedGraphs[graphName]=clone(graph());versions();status(`Published v${r.published} | run 104 · v3`);
  events.unshift([String(r.published),"Published",graphName,`v${r.published}`]);renderEvents();
}
window.__workflowMock={snapshot:()=>clone({graphName,graphs,revisions,selected,answered,live,runRevision:3}),openGraph};
renderShell();
