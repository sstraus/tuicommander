import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { it } from 'node:test';
import { createServer } from 'vite';
import solid from 'vite-plugin-solid';

// Real browser layout, not happy-dom's zero-size boxes. Screenshots clip only the fixture element.
it('catches shared control geometry drifting at desktop and touch widths', {timeout: 60000}, async (context) => {
  const directory = resolve(process.env.TUIC_GEOMETRY_EVIDENCE ?? '.tmp/control-geometry');
  await mkdir(directory, {recursive: true});
  const {version} = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
  const server = await createServer({configFile:false, define:{__APP_VERSION__:JSON.stringify(version),__BUILD_GIT_HASH__:JSON.stringify('geometry-fixture')}, plugins:[solid()], cacheDir:resolve('.tmp/control-geometry-vite'), logLevel:'error', server:{host:'127.0.0.1',port:0}});
  let chrome, socket;
  try {
    await server.listen();
    chrome = spawn(process.env.TUIC_TEST_CHROME, ['--headless=new', '--disable-gpu', '--no-first-run', '--remote-debugging-port=0', `--user-data-dir=${directory}/chrome`, 'about:blank'], {stdio:['ignore','ignore','pipe']});
    const endpoint = await new Promise((ok, fail) => {
      let stderr = '';
      chrome.stderr.on('data', data => {stderr += data; const match = stderr.match(/DevTools listening on (ws:\/\/\S+)/); if(match) ok(match[1]);});
      chrome.once('error', fail);
      chrome.once('exit', code => fail(new Error(`Chrome exited ${code}: ${stderr}`)));
    });
    socket = new WebSocket(endpoint);
    await new Promise((ok, fail) => {socket.addEventListener('open',ok,{once:true});socket.addEventListener('error',fail,{once:true});});
    let sequence=0;
    const pending = new Map();
    socket.addEventListener('close',()=>{for(const request of pending.values()){clearTimeout(request.timer);request.reject(new Error('Browser disconnected'));}pending.clear();});
    socket.addEventListener('message', event => {const message=JSON.parse(event.data);const request=pending.get(message.id);if(request){clearTimeout(request.timer);pending.delete(message.id);message.error ? request.reject(new Error(JSON.stringify(message.error))) : request.resolve(message.result);}});
    const send = (method, params={}, sessionId) => new Promise((resolve,reject) => {const id=++sequence;const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP ${method} did not respond`));},20000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,sessionId}));});
    const {targetId} = await send('Target.createTarget',{url:'about:blank'});
    const {sessionId} = await send('Target.attachToTarget',{targetId,flatten:true});
    const evaluate = async expression => {const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true},sessionId);if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
    await send('Page.enable',{},sessionId);
    const measurements=[];
    for(const width of (process.env.TUIC_GEOMETRY_WIDTHS ?? "1400,390").split(",").map(Number)) {
      const touch=(process.env.TUIC_GEOMETRY_FIXTURE ?? '').startsWith('mobile') && width<=600;
      await send('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:touch},sessionId);
      await send('Emulation.setTouchEmulationEnabled',{enabled:touch},sessionId);
      const loaded = new Promise((resolve,reject) => {const timer=setTimeout(()=>reject(new Error('Page load event did not arrive')),20000);const listener = event => {const message=JSON.parse(event.data);if(message.method==='Page.loadEventFired' && message.sessionId===sessionId){clearTimeout(timer);socket.removeEventListener('message',listener);resolve();}};socket.addEventListener('message',listener);});
      await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/src/__tests__/browser/${process.env.TUIC_GEOMETRY_FIXTURE ?? "controlGeometry"}.html${process.env.TUIC_GEOMETRY_SURFACE ? `?surface=${process.env.TUIC_GEOMETRY_SURFACE}` : ""}`},sessionId);
      await loaded;
      await evaluate(`new Promise((resolve,reject)=>{const stop=setTimeout(()=>reject(new Error('Fixture did not mount')),15000);const check=()=>{if(window.controlGeometryError){clearTimeout(stop);reject(new Error(window.controlGeometryError));return;}if(window.controlGeometryReady){clearTimeout(stop);requestAnimationFrame(()=>requestAnimationFrame(resolve));}else requestAnimationFrame(check);};check();})`);
      // The shared named class is the density contract, not a measured CSS value.
      // Consumer placement is separately asserted by densitySurfaces.
      // DEFERRED (2026-10-25) — pin the generic consumer inventory independently; it exceeds this dialog-boundary fix.
      await evaluate(`(async()=>{const s=(await import('/src/components/ui/FormControls.module.css')).default;window.controlGeometryDensity=element=>{for(let node=element;node;node=node.parentElement){if(node.classList.contains(s.form))return 'default';if(node.classList.contains(s.toolbar)||node.classList.contains(s.compact))return 'compact';}return 'default';};})()`);
      const result=await evaluate(`({rows:window.controlGeometryRows?window.controlGeometryRows():document.querySelector('[data-row]')?[...document.querySelectorAll('[data-row]')].map(row=>({size:row.dataset.row,heights:[...row.children].map(element=>({size:window.controlGeometryDensity(element),tag:element.tagName,name:element.getAttribute('aria-label')||element.getAttribute('title')||element.textContent?.trim(),height:element.getBoundingClientRect().height}))})):[{size:document.querySelector('#geometry').dataset.controlSize??'default',heights:[...document.querySelectorAll('#geometry [data-form-control]:not(textarea)')].map(element=>({size:window.controlGeometryDensity(element),tag:element.tagName,name:element.getAttribute('aria-label')||element.getAttribute('title')||element.textContent?.trim(),height:element.getBoundingClientRect().height}))}],clip:(()=>{const r=document.querySelector('#geometry').getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,scale:1};})()})`);
      // Catches: content-width icons, compact shrinking targets, rounded corner misses or lost native activation.
      const icons = await evaluate(`window.iconClicks === undefined ? [] : [...document.querySelectorAll('#geometry button')].map(button=>{const r=button.getBoundingClientRect();return {label:button.getAttribute('aria-label'),width:r.width,height:r.height,x:r.x+1,y:r.y+1,corner:button.contains(document.elementFromPoint(r.x+1,r.y+1)),radius:getComputedStyle(button).borderRadius,circle:button.hasAttribute('data-circle')?(button.firstElementChild?getComputedStyle(button.firstElementChild).borderRadius:'none'):null,disabled:button.disabled};})`);
      for(const icon of icons) {
        assert.equal(icon.width,width<=600?44:36,'Icon target must be square, not content width');
        assert.equal(icon.height,width<=600?44:36,'Compact must not shrink an icon target');
        assert.ok(icon.corner,'Circular presentation must retain the square corner hit region');
        if(icon.circle) {assert.equal(icon.radius,'0px');assert.equal(icon.circle,'50%');}
      }
      if(icons.length) {
        for(const icon of [icons[1],icons[3]]) {
          const before=await evaluate('window.iconClicks');
          await send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,x:icon.x,y:icon.y},sessionId);
          await send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,x:icon.x,y:icon.y},sessionId);
          assert.equal(await evaluate('window.iconClicks'),before+(icon.disabled?0:1),'Native corner activation respects disabled');
        }
        await evaluate(`document.querySelector('[data-circle]').focus()`);
        await send('Input.dispatchKeyEvent',{type:'keyDown',key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        await send('Input.dispatchKeyEvent',{type:'keyUp',key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.equal(await evaluate('window.iconClicks'),2,'Space must activate the native focused icon');
        assert.equal(await evaluate(`getComputedStyle(document.activeElement).outlineStyle`),'solid','Native focus remains visible');
        assert.equal(await evaluate(`getComputedStyle(document.querySelector('[aria-label="Primary help"]').firstElementChild).backgroundColor`),'rgb(10, 20, 200)','Circle retains primary palette');
        assert.equal(await evaluate(`getComputedStyle(document.querySelector('[aria-label="Remove help"]').firstElementChild).borderColor`),'rgb(10, 200, 20)','Circle retains pressed removal state');
        const last=icons.at(-1);
        await send('Input.dispatchMouseEvent',{type:'mouseMoved',x:last.x+last.width/2-1,y:last.y+last.height/2-1},sessionId);
        assert.equal(await evaluate(`getComputedStyle(document.querySelector('[aria-label="Remove help"]').firstElementChild).borderColor`),'rgb(200, 10, 20)','Circle retains removal hover');

      }
      // Catches: a migrated icon action retaining a content-width hit target.
      const actionIcons = await evaluate(`(window.geometryIconLabels ?? []).map(label=>{const button=[...document.querySelectorAll('#geometry button')].find(button=>(button.getAttribute('aria-label')||button.getAttribute('title'))===label);if(!button)throw new Error('Missing expected icon '+label);const r=button.getBoundingClientRect();return {label,size:window.controlGeometryDensity(button),width:r.width,height:r.height};})`);
      for(const icon of actionIcons) {
        assert.equal(icon.width,width<=600?44:icon.size==='compact'?22:36,JSON.stringify(icon));
        assert.equal(icon.height,width<=600?44:icon.size==='compact'?22:36,JSON.stringify(icon));
      }
      const stretch = await evaluate(`[...document.querySelectorAll('[data-stretch-case]')].map(row=>({container:row.dataset.stretchCase,controls:[...row.querySelectorAll('button')].map(button=>({height:button.getBoundingClientRect().height,width:button.getBoundingClientRect().width,radius:getComputedStyle(button).borderRadius}))}))`);
      for (const row of stretch) {
        assert.ok(row.controls.length >= 2, 'Stretch cases must exercise multiple button sizes');
        for (const control of row.controls) {
          assert.equal(control.height, Math.max(Number(row.container), width <= 600 ? 44 : 36), JSON.stringify({width,row}));
          assert.ok(control.width >= (width <= 600 ? 44 : 36), JSON.stringify({width,row}));
          assert.equal(control.radius, '0px', 'Stretch actions must have square edges');
        }
      }
      const joined = await evaluate(`[...document.querySelectorAll('[data-joined-case]')].map(group=>({size:group.dataset.joinedCase,overflow:getComputedStyle(group).overflow,controls:[...group.querySelectorAll('button')].map(button=>{const r=button.getBoundingClientRect(),s=getComputedStyle(button);return {left:r.left,right:r.right,height:r.height,start:s.borderStartStartRadius,end:s.borderStartEndRadius,startBorder:s.borderInlineStartWidth,endBorder:s.borderInlineEndWidth};})}))`);
      for (const group of joined) {
        assert.notEqual(group.overflow,'hidden','Group must not clip focus or dropdown');
        assert.ok(group.controls.length > 0, 'Joined group must mount real buttons');
        for (const [index,control] of group.controls.entries()) {
          assert.equal(control.height, width<=600?44:group.size==='compact'?22:36);
          if(index>0) {
            assert.equal(control.left,group.controls[index-1].right,'Connected edges must meet');
            assert.equal(control.start,'0px');
            assert.equal(control.startBorder,'1px','Join must have exactly one divider');
          } else assert.notEqual(control.start,'0px','First action retains outer radius');
          if(index<group.controls.length-1) {assert.equal(control.end,'0px');assert.equal(control.endBorder,'0px');}
          else assert.notEqual(control.end,'0px','Last action retains outer radius');
        }
      }
      const textarea = await evaluate('window.textareaGeometry ? window.textareaGeometry() : null');
      if(textarea) {
        const minimum=width<=600?44:36, maximum=width<=600?132:108;
        assert.equal(textarea.cleared.height,minimum,'Cleared auto-grow must shrink to shared target');
        assert.equal(textarea.long.height,maximum,'Long content must respect shared cap');
        assert.ok(textarea.long.scroll>textarea.long.client,'Long content must remain scrollable');
        assert.equal(textarea.long.overflow,'auto');
        assert.ok(textarea.narrow.height>textarea.wide.height,'Width-only wrapping must resize without input');
        for(const name of ['initial','replaced'])assert.ok(textarea[name].height>minimum && textarea[name].height<maximum,JSON.stringify({name,textarea}));
      }
      await send('Input.dispatchMouseEvent',{type:'mouseMoved',x:0,y:0},sessionId);
      const states = await evaluate('window.controlStateGeometry ? window.controlStateGeometry() : null');
      if(states) {
        for(const control of states.invalid)assert.equal(control.border,'rgb(200, 10, 20)','Native invalid field must use error token');
        for(const control of states.focused) {assert.equal(control.border,'rgb(200, 10, 20)');assert.equal(control.outline,'rgb(10, 20, 200)');assert.equal(control.outlineStyle,'solid');}
        for(const control of states.valid)assert.equal(control.border,'rgb(80, 80, 80)','False invalid state must reset border');
        assert.equal(states.off.border,'rgb(80, 80, 80)');assert.equal(states.on.border,'rgb(10, 200, 20)');
        assert.equal(states.disabled.border,'rgb(10, 200, 20)');assert.equal(states.selection.border,'rgb(10, 20, 200)','Ordinary selected actions retain accent');
        for(const [selector,expected] of [['[data-remove-toggle]','rgb(200, 10, 20)'],['[data-disabled-toggle]','rgb(10, 200, 20)']]) {
          const box=await evaluate(`(()=>{const r=document.querySelector('${selector}').getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);
          await send('Input.dispatchMouseEvent',{type:'mouseMoved',...box},sessionId);
          const color=await evaluate(`getComputedStyle(document.querySelector('${selector}')).borderColor`);
          assert.equal(color,expected,'Enabled removal hover only must use danger token');
        }
      }
      // Catches: palette Button migration dropping draggable/onDragStart or adding a node twice.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'workflowDesignerGeometry') {
        const before = await evaluate(`document.querySelectorAll('[data-node-id]').length`);
        // The touch sidebar scrolls independently above the inspector; use its visible hit target.
        await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='Add Notify').scrollIntoView({block:'center'});new Promise(resolve=>requestAnimationFrame(resolve))`);
        const boxes = await evaluate(`(()=>{const source=[...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='Add Notify').getBoundingClientRect();const target=document.querySelector('[aria-label="Workflow graph"]').getBoundingClientRect();return {source:{x:source.x+source.width/2,y:source.y+source.height/2},target:{x:target.x+target.width/2,y:target.y+target.height/2}};})()`);
        console.log('Palette drag bounds',JSON.stringify(boxes));
        await evaluate(`window.workflowDragEvents=[];for(const type of ['mousedown','dragstart','dragend'])document.addEventListener(type,event=>window.workflowDragEvents.push({type,text:event.target.textContent,draggable:event.target.draggable}),{capture:true});`);
        await send('Input.setInterceptDrags',{enabled:true},sessionId);
        const intercepted = new Promise((resolve,reject)=>{
          const listener=event=>{const message=JSON.parse(event.data);if(message.method==='Input.dragIntercepted' && message.sessionId===sessionId){clearTimeout(timer);socket.removeEventListener('message',listener);resolve(message.params.data);}};
          const timer=setTimeout(()=>{socket.removeEventListener('message',listener);reject(new Error('Native palette drag did not start'));},10000);
          socket.addEventListener('message',listener);
        });
        await send('Input.dispatchMouseEvent',{type:'mouseMoved',...boxes.source},sessionId);
        await send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,...boxes.source},sessionId);
        await evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
        for(let step=1;step<=8;step++) {
          await send('Input.dispatchMouseEvent',{type:'mouseMoved',button:'left',buttons:1,x:boxes.source.x+(boxes.target.x-boxes.source.x)*step/8,y:boxes.source.y+(boxes.target.y-boxes.source.y)*step/8},sessionId);
          await evaluate(`new Promise(resolve=>requestAnimationFrame(resolve))`);
        }
        console.log('Palette drag events',JSON.stringify(await evaluate('window.workflowDragEvents')));
        const data=await intercepted;
        assert.equal(data.items.find(item=>item.mimeType==='application/x-tuic-node')?.data,'notify:','Use the real palette drag payload, not a fabricated DataTransfer');
        for(const type of ['dragEnter','dragOver','drop'])await send('Input.dispatchDragEvent',{type,...boxes.target,data},sessionId);
        await send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,...boxes.target},sessionId);
        await send('Input.setInterceptDrags',{enabled:false},sessionId);
        assert.equal(await evaluate(`document.querySelectorAll('[data-node-id]').length`),before+1,'One native drop adds exactly one graph node');
        assert.equal(await evaluate(`document.querySelector('[data-node-id="notify_1"]').getAttribute('aria-pressed')`),'true','The dropped node becomes selected');
        // Native Space must still activate the palette click handler once.
        await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='Add Loop').focus()`);
        for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.equal(await evaluate(`document.querySelectorAll('[data-node-id]').length`),before+2,'Space adds exactly one further node');
        assert.equal(await evaluate(`document.querySelector('[data-node-id="loop_1"]').getAttribute('aria-pressed')`),'true');
        assert.equal(await evaluate(`getComputedStyle(document.activeElement).outlineStyle`),'solid','Native keyboard focus stays visible');
        console.log(`Workflow palette native drag and Space activation passed at ${width}px`);
      }
      // Catches: a migrated variable trigger loses native keyboard activation or leaves its popup latched.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'smartPromptSettingsGeometry') {
        await evaluate(`[...document.querySelectorAll('#geometry button')].find(button=>button.textContent.includes('Insert variable...')).focus()`);
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.equal(await evaluate(`document.querySelectorAll('#geometry button[data-control-layout="row"]').length`),0,'Space closes variable rows');
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.ok(await evaluate(`document.querySelectorAll('#geometry button[data-control-layout="row"]').length>10`),'Space opens the native variable list');
      }
      // Catches: a migrated repository switch loses native Space activation or an action loses Enter.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'repositoryGeometry') {
        await evaluate(`document.querySelector('#geometry input[role="switch"]').focus()`);
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.equal(await evaluate(`document.querySelector('#geometry input[role="switch"]').checked`),true,'Space enables consolidation through its owner');
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.equal(await evaluate(`document.querySelector('#geometry input[role="switch"]').checked`),false,'Space disables consolidation through its owner');
        await evaluate(`[...document.querySelectorAll('#geometry button')].find(button=>button.textContent.includes('Copy settings')).focus()`);
        assert.ok(await evaluate(`document.activeElement.textContent.includes('Copy settings')`),'Export action receives keyboard focus');
        await send('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,text:'\r'},sessionId);
        await send('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13},sessionId);
        assert.deepEqual(await evaluate('window.repositoryActions'),{exports:1,resets:0},'Export activates only the existing export callback');
      }
      // Catches: removing Telegram's panel sizing changes disabled or native keyboard behavior.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'telegramGeometry') {
        assert.deepEqual(await evaluate(`[...document.querySelectorAll('#geometry button:disabled')].map(button=>button.textContent.trim()).sort()`),['Add chat ID','Save and check bot']);
        await evaluate(`[...document.querySelectorAll('#geometry button')].find(button=>button.textContent.includes('Reload status')).focus()`);
        const reads=await evaluate('window.telegramReads');
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.ok(await evaluate('window.telegramReads')>reads,'Space reloads status through the existing owner');
      }
      // Catches: native overlay/disclosure exceptions shrinking or clipping shared AI Chat targets.
      const aiChat = await evaluate('window.aiChatGeometry ? window.aiChatGeometry() : null');
      if (aiChat) {
        const target = width <= 600 ? 44 : 22;
        assert.deepEqual(aiChat.rawActions, [], 'AI Chat actions and text fields must use shared primitives');
        assert.equal(aiChat.visible.height, target, 'Visible picker frame owns shared target height');
        assert.deepEqual(aiChat.picker, aiChat.visible, 'Invisible native picker must cover the complete visible frame');
        assert.equal(aiChat.summaries.length, 2, 'Both native SessionControls disclosures must mount');
        for (const [index,summary] of aiChat.summaries.entries()) assert.equal(summary.height, width <= 600 ? 44 : index === 0 ? 22 : 36, JSON.stringify(summary));
        assert.equal(aiChat.summaries[0].width, target, 'Icon disclosure target stays square');
        assert.ok(aiChat.summaries[0].top >= aiChat.header.top && aiChat.summaries[0].bottom <= aiChat.header.bottom,
          'Header must contain the shared-height disclosure');
        assert.ok(aiChat.editor.height >= target, 'Composer must retain a complete editable line');
        assert.equal(aiChat.editorText, 'A complete composing line');
        await evaluate(`document.querySelector('summary[data-control-layout="disclosure"]').parentElement.open=true; document.querySelectorAll('summary[data-control-layout="disclosure"]')[1].focus()`);
        await send('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
        await send('Input.dispatchKeyEvent',{type:'keyUp',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
        assert.equal(await evaluate(`document.querySelector('summary[data-control-layout="disclosure"]').parentElement.open`),false,'Escape from nested disclosure must close the actions menu');
        assert.equal(await evaluate(`document.activeElement === document.querySelector('summary[data-control-layout="disclosure"]')`),true,'Escape must return focus to the native actions summary');
        await evaluate(`document.querySelector('summary[data-control-layout="disclosure"]').parentElement.open=true`);
      }
      // Catches: desktop composer overflow hiding the phone editor's long draft from touch scrolling.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'mobileChatScreenGeometry' && width <= 600) {
        const editor = await evaluate(`(async()=>{const field=document.querySelector('textarea');field.value=Array.from({length:20},(_,i)=>'Draft line '+i).join('\\n');field.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));const style=getComputedStyle(field);return {scroll:field.scrollHeight,client:field.clientHeight,overflow:style.overflowY};})()`);
        await context.test('catches long phone AI Chat drafts losing native scrolling', () => {
          assert.ok(editor.scroll > editor.client, 'The draft must exceed the phone editor cap');
          assert.equal(editor.overflow, 'auto', 'Long phone drafts must remain natively scrollable');
        });
      }
      // Catches: enlarged queue actions consuming the compose editor's fixed-height panel.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'composeGeometry') {
        const draft = await evaluate(`(()=>{const editor=document.querySelector('[data-compose-editor]');const line=editor?.querySelector('.cm-line');if(!editor||!line)throw new Error('Compose fixture must mount a real draft line');const viewport=editor.getBoundingClientRect(),text=line.getBoundingClientRect();return {viewportTop:viewport.top,viewportBottom:viewport.bottom,lineTop:text.top,lineBottom:text.bottom};})()`);
        await context.test(`catches queue review clipping the composing draft at ${width}px`, () => {
          assert.ok(draft.lineTop >= draft.viewportTop && draft.lineBottom <= draft.viewportBottom,
            `Queue review must leave a complete draft line visible: ${JSON.stringify(draft)}`);
        });
      }
      result.containment = [];
      const composer=await evaluate("window.composerGeometry ?? null");
      const targets=await evaluate(`[...document.querySelectorAll('#geometry button')].map(button=>{const r=button.getBoundingClientRect();return {name:button.getAttribute('aria-label')||button.title||button.textContent.trim(),width:r.width,height:r.height,borderColor:getComputedStyle(button).borderTopColor,ready:button.hasAttribute('data-ready'),disabled:button.disabled,iconOnly:button.hasAttribute('data-form-control') && (!button.textContent.trim() || /^[+−×✕]$/.test(button.textContent.trim()))};})`);
      // Catches: a default content-width Button leaves mobile icon actions narrower than44px.
      if((process.env.TUIC_GEOMETRY_FIXTURE ?? '').startsWith('mobile') && width<=600) {
        for(const target of targets.filter(target=>target.iconOnly && target.width>0 && target.height>0)) {
          assert.equal(target.width,44,JSON.stringify({width,target}));
          assert.equal(target.height,44,JSON.stringify({width,target}));
        }
      }
      measurements.push({width,...result,icons,actionIcons,stretch,joined,textarea,states,composer,targets,aiChat});

      const image=await send('Page.captureScreenshot',{format:'png',clip:result.clip},sessionId);
      const path=resolve(directory,`controls-${width}.png`);
      await writeFile(path,Buffer.from(image.data,'base64'));
      console.log(path);
      // Catches: shared Rebind/Clear lose native activation or cancellation changes a binding.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'workspaceGeometry') {
        const binding=await evaluate(`document.querySelector('#geometry tr kbd').textContent`);
        await evaluate(`document.querySelector('#geometry button[title="Edit shortcut"]').focus()`);
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.equal(await evaluate(`document.querySelector('#geometry tr [tabindex="0"]').textContent`),'Press a key...','Space enters the real shortcut recorder');
        // Drive cancellation explicitly; automatic recorder focus is not asserted by this geometry fixture.
        await evaluate(`document.querySelector('#geometry tr [tabindex="0"]').focus()`);
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
        assert.equal(await evaluate(`document.querySelector('#geometry tr kbd').textContent`),binding,'Escape retains the existing binding');
        await evaluate(`[...document.querySelectorAll('#geometry button')].find(button=>button.textContent.trim()==='Clear').focus()`);
        for (const type of ['keyDown','keyUp']) await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32},sessionId);
        assert.deepEqual(await evaluate('window.workspaceHotkeys'),['Super+Shift+Y',null],'Clear calls the settings owner with null');
        assert.equal(await evaluate(`[...document.querySelectorAll('#geometry button')].some(button=>button.textContent.trim()==='Clear')`),false,'Cleared hotkey removes its conditional action');
      }
      // Catches: platform-specific desktop height wins over wrapped touch rows.
      if (process.env.TUIC_GEOMETRY_FIXTURE === 'toolbarGeometry') {
        for (const platform of ['linux', 'windows']) {
          await evaluate(`document.documentElement.className='platform-${platform}'; new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
          const bounds = await evaluate(`(()=>{const toolbar=document.querySelector('#toolbar'); const box=toolbar.getBoundingClientRect(); const buttons=[...toolbar.querySelectorAll('button')]; return {toolbarBottom:box.bottom,actionsBottom:Math.max(...buttons.map(button=>button.getBoundingClientRect().bottom)),actions:buttons.length};})()`);
          result.containment.push({platform,...bounds});
          console.log('CONTAINMENT',JSON.stringify({width,platform,...bounds}));
          const capture = await send('Page.captureScreenshot',{format:'png',clip:(await evaluate(`(()=>{const box=document.querySelector('#geometry').getBoundingClientRect(); return {x:box.x,y:box.y,width:box.width,height:Math.max(box.height,100),scale:1};})()`))},sessionId);
          const platformPath=resolve(directory,`controls-${width}-${platform}.png`);
          await writeFile(platformPath,Buffer.from(capture.data,'base64'));
          console.log(platformPath);
        }
        await evaluate(`document.documentElement.className=''`);
      }
    }
    await writeFile(resolve(directory,'measurements.json'),JSON.stringify(measurements,null,2));
    if (process.env.TUIC_GEOMETRY_FIXTURE === 'toolbarGeometry') {
      for (const result of measurements) assert.equal(result.containment.length, 2, 'Both platform containment cases must execute');
    }
    for (const result of measurements) for (const bounds of result.containment) {
      assert.ok(bounds.actions >= 2, 'Containment must measure actual mounted toolbar actions');
      assert.ok(bounds.actionsBottom <= bounds.toolbarBottom + 1, `${bounds.platform} toolbar must contain actions at ${result.width}px: ${JSON.stringify(bounds)}`);
    }
    for(const result of measurements)for(const row of result.rows) {
      assert.ok(row.heights.length >= 2, 'The mounted fixture must contain multiple real controls');
      for(const element of row.heights) {
        const minimum=result.width<=600?44:(element.size ?? row.size)==='compact'?22:36;
        if(row.size==='stretch') {
          assert.ok(element.height>=minimum,'Stretch actions retain the row density minimum');
          assert.equal(element.height,row.containerHeight,'Stretch actions fill the tab strip, not a third fixed density');
        } else assert.equal(element.height,minimum,JSON.stringify({width:result.width,row}));
      }
    }
  } finally {
    socket?.close();
    if(chrome && chrome.exitCode === null) {chrome.kill();await new Promise(resolve=>chrome.once('exit',resolve));}
    await server.close();
  }
});
