// Isolated desktop UI acceptance. No personal browser profile, microphone, or installed app.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,writeFile,mkdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve,extname} from 'node:path';
import http from 'node:http';
import WebSocket from '../web/node_modules/ws/wrapper.mjs';
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const platform=process.env.ADMIN_TEST_PLATFORM||'macos';
assert(['macos','windows'].includes(platform),'supported UI fixture');
const outputName=platform==='macos'?'BlackHole 2ch':'CABLE Input',shortcut=platform==='macos'?'fn':'hid_right_alt';
const token='isolated-ui-test',owner='doubao-microphone-admin-token';
const ids=['virtual_microphone','doubao_installation','doubao_microphone','doubao_trigger','doubao_voice_switch','input_source','accessibility'];
const titles=['虚拟麦克风','豆包安装及运行','豆包麦克风配置','长按快捷键配置','语音快捷键开关','当前输入法','辅助功能权限'];
const device={id:'12345678-0000-4000-8000-000000000001',label:'iPhone App'};
const state={fail:false,forceRestart:false,paired:null,tickets:0,unauthorized:0,devices:[device],interfaces:[{name:'en0',ip:'192.168.3.8',ethernet:false}],status:{phase:'idle',connected:false,device_id:null,paused:false,input_state:'ready',input_recovery_attempts:0,mock:false,queued_ms:0,error:null,session_id:null,testing:false},config:{computer_name:'测试电脑',network_interface:null,lan_ip:'192.168.3.8',output_device:outputName,shortcut,shortcut_manual:false,warmup_ms:350,enter_delay_ms:1000,autostart:false,diagnostic_recording:false,codex_empty_compatibility:false}};
const qr='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 25"><rect width="25" height="25" fill="white"/>'+Array.from({length:21},(_,y)=>Array.from({length:21},(_,x)=>(x*y+x+y)%3===0?`<rect x="${x+2}" y="${y+2}" width="1" height="1"/>`:'').join('')).join('')+'</svg>';
const detail='请在系统设置中检查并允许豆包麦克风。'+('这里是完整的检测说明，详细信息可以换行查看。'.repeat(60));
const server=http.createServer(async(req,res)=>{
 try{
  const path=new URL(req.url,'http://localhost').pathname;
  if(path.startsWith('/api/')){
   if(req.headers.authorization!=='Bearer '+token){state.unauthorized++;res.writeHead(401);res.end('{}');return}
   let data='';for await(const chunk of req)data+=chunk;const body=data?JSON.parse(data):null;let value={};
   if(path==='/api/status')value={status:state.status,paired_ticket:state.paired};
   else if(path==='/api/devices')value=state.devices;
   else if(path.startsWith('/api/devices/')){
    const item=state.devices.find(d=>d.id===path.split('/').at(-1));
    if(req.method==='PATCH'){item.label=body.label.trim();value={renamed:true}}
    else {state.devices=state.devices.filter(d=>d!==item);value={revoked:true}}
   }else if(path==='/api/config'){
    if(req.method==='PUT'){state.config=body;value={saved:true,restart_required:state.forceRestart,shortcut:body.shortcut,lan_ip:body.lan_ip,network_interface:body.network_interface}}
    else value=state.config;
   }else if(path==='/api/setup')value={platform:{platform,native_supported:true,accessibility:true,release_enter:true,automatic_shortcut:shortcut,shortcut_locked:true,shortcuts:[shortcut]},interfaces:state.interfaces,outputs:state.outputs??[{id:'blackhole',name:outputName,virtual_candidate:true}],ca_sha256:'00:11:22'};
   else if(path==='/api/environment')value={supported:true,checks:ids.map((id,i)=>({id,title:titles[i],state:state.fail?'fail':'pass',detail:state.fail?detail:'检查通过'}))};
   else if(path==='/api/pair-ticket'){state.paired=null;value={ticket_id:'ticket-'+(++state.tickets),qr_svg:qr,url:'http://127.0.0.1/setup',phone_url:'https://127.0.0.1/',expires_in:300}}
   else if(path==='/api/pause'){state.status.paused=body.paused;value={paused:body.paused}}
   res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));return;
  }
  const filename=path==='/'?'admin.html':path==='/settings'?'settings.html':path==='/phone'?'phone.html':path.slice(1);
  if(filename.includes('..')){res.writeHead(400);res.end();return}
  const file=await readFile(resolve('web/dist',filename));res.setHeader('Content-Type',({'.html':'text/html','.js':'text/javascript','.css':'text/css'})[extname(filename)]||'application/octet-stream');res.end(file);
 }catch(error){res.writeHead(500);res.end(String(error))}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base='http://127.0.0.1:'+server.address().port;
const dir=await mkdtemp(join(tmpdir(),'doubao-admin-ui-')),profile=join(dir,'chrome');await mkdir(profile);
const chrome=process.env.CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser=spawn(chrome,['--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check','--disable-background-networking','--remote-debugging-port=0','--remote-allow-origins=*',`--user-data-dir=${profile}`,'about:blank'],{stdio:'ignore'});
let cdp;const errors=[],dialogs=[];const artifacts=resolve(platform==='macos'?'artifacts/admin-ui':'artifacts/admin-ui/windows');await mkdir(artifacts,{recursive:true});
try{
 let address;
 for(let n=0;n<150;n++){try{const [port,path]=(await readFile(join(profile,'DevToolsActivePort'),'utf8')).trim().split('\n');address=`ws://127.0.0.1:${port}${path}`;break}catch{await sleep(100)}}
 assert(address,'isolated Chrome startup');cdp=new WebSocket(address);await new Promise((r,j)=>{cdp.once('open',r);cdp.once('error',j)});
 let next=1;const pending=new Map();
 cdp.on('message',buffer=>{const m=JSON.parse(buffer.toString());if(m.id){const p=pending.get(m.id);if(p){pending.delete(m.id);clearTimeout(p.timer);m.error?p.reject(Error(m.error.message)):p.resolve(m.result)}}else if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails.exception?.description||m.params.exceptionDetails.text);else if(m.method==='Page.javascriptDialogOpening')dialogs.push(m.params)});
 function call(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('timeout: '+method))},15000);pending.set(id,{resolve,reject,timer});cdp.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}))})}
 const {targetId}=await call('Target.createTarget',{url:'about:blank'}),{sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
 const command=(method,params={})=>call(method,params,sessionId);
 const evaluate=async(expression,userGesture=false)=>{const result=await command('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true,userGesture});if(result.exceptionDetails)throw Error(result.exceptionDetails.exception?.description||result.exceptionDetails.text);return result.result.value};
 const wait=async(expression,label=expression)=>{for(let n=0;n<120;n++){if(await evaluate(expression))return;await sleep(50)}throw Error('UI wait failed: '+label)};
 const navigate=async(path)=>{await command('Page.navigate',{url:base+path});await wait(`location.pathname===${JSON.stringify(path.split('#')[0])}`)};
 const homeReady=()=>wait(`document.querySelectorAll('#environment-checks [data-state="pass"]').length===7&&document.querySelector('#qr svg')!==null`,'home ready');
 const settingsReady=()=>wait(`document.querySelector('#settings')?.dataset.ready==='true'&&document.querySelector('#save-settings')?.disabled===false`,'settings ready');
 const bounds=async()=>{const b=await evaluate('({height:document.documentElement.scrollHeight,width:document.documentElement.scrollWidth,innerHeight,innerWidth})');if(b.height>b.innerHeight){const shot=await command('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});await writeFile(join(artifacts,'overflow.png'),Buffer.from(shot.data,'base64'));console.error(await evaluate(`Array.from(document.querySelectorAll('.admin-header,.admin-alert,.dashboard-card,.environment-row')).map(e=>({class:e.className,hidden:e.hidden,height:e.getBoundingClientRect().height}))`))}assert(b.height<=b.innerHeight,JSON.stringify(b));assert(b.width<=b.innerWidth,JSON.stringify(b))};
 const screenshot=async(name)=>{const image=await command('Page.captureScreenshot',{format:'png'});await writeFile(join(artifacts,name),Buffer.from(image.data,'base64'))};
 await command('Runtime.enable');await command('Page.enable');await command('Emulation.setDeviceMetricsOverride',{width:1280,height:720,deviceScaleFactor:1,mobile:false});
 await navigate('/#token='+token);await homeReady();assert.equal(state.tickets,1);await bounds();await screenshot('homepage.png');
 const homeFonts=await evaluate(`({title:getComputedStyle(document.querySelector('.card-heading h2')).fontSize,label:getComputedStyle(document.querySelector('.environment-row-title')).fontSize,button:getComputedStyle(document.querySelector('.card-heading button')).fontSize})`);
 assert.equal(await evaluate('location.hash'),'');
 const first=await evaluate(`JSON.parse(sessionStorage.getItem('doubao-microphone-pairing')).ticket.ticket_id`);
 await evaluate(`document.querySelector('a[href="/settings"]').click()`,true);await settingsReady();
 assert.equal(await evaluate(`document.querySelector('#network-choice-row').hidden`),true);
 assert.equal(await evaluate(`document.querySelector('#network-summary').value`),'自动选择（en0）');
 assert.equal(await evaluate(`document.querySelector('#advanced').hidden`),true);
 assert.equal(await evaluate(`document.querySelector('#settings-footer').hidden`),true);
 assert.equal(await evaluate(`document.querySelector('#output_device').hidden`),true);
 assert.equal(await evaluate(`getComputedStyle(document.querySelector('.settings-panel h2')).fontSize`),homeFonts.title);
 assert.equal(await evaluate(`getComputedStyle(document.querySelector('.setting-row')).fontSize`),homeFonts.label);
 assert.equal(await evaluate(`getComputedStyle(document.querySelector('#refresh')).fontSize`),homeFonts.button);
 await bounds();await screenshot('settings-general.png');
 await evaluate(`document.querySelector('#computer_name').value='尚未保存';document.querySelector('#computer_name').dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('[href="#advanced"]').click()`,true);
 await wait(`!document.querySelector('#advanced').hidden`);
 assert.equal(await evaluate(`document.querySelector('#general-panel').hidden`),true);
 assert.equal(await evaluate(`document.querySelector('#settings-footer').hidden`),false);
 assert.equal(await evaluate(`document.querySelectorAll('#advanced details[open]').length`),0);
 await bounds();await screenshot('settings-advanced.png');
 await evaluate(`document.querySelector('[href="#general"]').click()`,true);await wait(`!document.querySelector('#general-panel').hidden`);
 assert.equal(await evaluate(`document.querySelector('#computer_name').value`),'尚未保存');
 await evaluate(`document.querySelector('#discard-settings').click()`,true);await wait(`document.querySelector('#computer_name').value==='测试电脑'&&document.querySelector('#settings-footer').hidden`);
 await evaluate(`document.querySelector('[href="#devices"]').click()`,true);await wait(`!document.querySelector('#devices-section').hidden`);await screenshot('settings-devices.png');
 await evaluate(`document.querySelector('[href="#web-help"]').click()`,true);await wait(`!document.querySelector('#web-help').hidden`);await screenshot('settings-help.png');
 await evaluate(`document.querySelector('[data-home-link]').click()`,true);await homeReady();assert.equal(state.tickets,1);assert.equal(await evaluate(`JSON.parse(sessionStorage.getItem('doubao-microphone-pairing')).ticket.ticket_id`),first);
 // A paired device removes the QR; arbitrarily many devices cannot enlarge the home page.
 state.paired=first;state.status.connected=true;state.status.device_id=device.id;
 await wait(`document.querySelector('#qr').textContent.includes('配对成功')`);await bounds();await screenshot('paired.png');
 state.devices=Array.from({length:30},(_,i)=>({...device,id:i?`${String(i).padStart(8,'0')}-0000-4000-8000-000000000001`:device.id}));
 await wait(`document.querySelector('#device-count').textContent.includes('30')`);await bounds();
 await evaluate(`document.querySelector('#ticket').click()`,true);await wait(`document.querySelector('#qr svg')!==null`);assert.equal(state.tickets,2);
 await evaluate(`{const key='doubao-microphone-pairing',s=JSON.parse(sessionStorage.getItem(key));s.ticket.expiresAt=Date.now()-1000;sessionStorage.setItem(key,JSON.stringify(s))}`);
 await navigate('/');await wait(`document.querySelector('#qr').textContent.includes('已过期')`);assert.equal(state.tickets,2);await bounds();await screenshot('expired.png');
 await evaluate(`document.querySelector('#ticket').click()`,true);await homeReady();assert.equal(state.tickets,3);
 // Multiple/absent NICs, dirty form protection, successful return, and restart warning.
 state.interfaces.push({name:'en5',ip:'10.0.0.2',ethernet:true});await navigate('/settings');await settingsReady();
 assert.equal(await evaluate(`document.querySelector('#network-choice-row').hidden`),false);
 assert.equal(await evaluate(`document.querySelector('#network_interface').options.length`),3);
 await evaluate(`document.querySelector('#computer_name').value='新的电脑名称';document.querySelector('#computer_name').dispatchEvent(new Event('input',{bubbles:true}))`,true);
 let count=dialogs.length,click=evaluate(`document.querySelector('[data-home-link]').click()`,true);
 for(let n=0;n<100&&dialogs.length===count;n++)await sleep(20);assert(dialogs.length>count,'unsaved confirmation');await command('Page.handleJavaScriptDialog',{accept:false});await click;
 assert.equal(await evaluate('location.pathname'),'/settings');assert.equal(await evaluate(`document.querySelector('#computer_name').value`),'新的电脑名称');
 await evaluate(`document.querySelector('#save-settings').click()`,true);await homeReady();assert.equal(state.config.computer_name,'新的电脑名称');assert.equal(state.tickets,3);
 state.forceRestart=true;state.interfaces=[];await navigate('/settings');await settingsReady();
 assert.equal(await evaluate(`document.querySelector('#network-summary').value`),'自动选择（暂无可用网络）');
 await evaluate(`document.querySelector('#autostart').click();document.querySelector('#save-settings').click()`,true);await wait(`document.querySelector('#notice').textContent.includes('重启')`);assert.equal(await evaluate('location.pathname'),'/settings');
 state.status.session_id='active';await wait(`document.querySelector('#computer_name').disabled`);assert.equal(await evaluate(`document.querySelector('#save-settings').disabled`),true);state.status.session_id=null;await settingsReady();
 // Device management and help anchors reuse authentication.
 await navigate('/settings#devices');await settingsReady();assert.equal(await evaluate('location.hash'),'#devices');
 count=dialogs.length;click=evaluate(`document.querySelector('#device-list button').click()`,true);
 for(let n=0;n<100&&dialogs.length===count;n++)await sleep(20);assert(dialogs.length>count,'rename prompt');await command('Page.handleJavaScriptDialog',{accept:true,promptText:'工作手机'});await click;
 await wait(`document.querySelector('#device-list').textContent.includes('工作手机')`);
 const pairedCount=state.devices.length;
 await evaluate(`document.querySelector('#device-list .device-actions button:last-child').click()`,true);
 await wait(`document.querySelectorAll('#device-list [data-device-id]').length===${pairedCount-1}`);
 assert.equal(state.devices.length,pairedCount-1,'device revoked');
 await navigate('/settings#web-help');await settingsReady();await wait(`!document.querySelector('#web-help').hidden`);assert.equal(await evaluate(`document.querySelector('#devices-section').hidden`),true);
 state.forceRestart=false;state.fail=true;state.status.error=detail;await navigate('/');await wait(`document.querySelectorAll('#environment-checks [data-state="fail"]').length===7`);await wait(`!document.querySelector('#error').hidden`);await bounds();await screenshot('errors.png');
 await evaluate(`document.querySelector('.check-details').click()`,true);assert.equal(await evaluate(`document.querySelector('#detail-text').textContent`),detail);assert.equal(await evaluate(`document.querySelector('#detail-dialog').open`),true);await evaluate(`document.querySelector('#detail-close').click()`);
 await command('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true);await screenshot('small-window.png');
 await navigate('/settings');await settingsReady();assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true);await screenshot('settings-small-window.png');
 await evaluate(`document.querySelector('[href="#advanced"]').click()`,true);await wait(`!document.querySelector('#advanced').hidden`);assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true);
 await command('Emulation.setDeviceMetricsOverride',{width:1280,height:720,deviceScaleFactor:1,mobile:false});
 // More than one usable audio device exposes the selector.
 state.outputs=[{id:'blackhole',name:outputName,virtual_candidate:true},{id:'mock',name:'模拟设备',virtual_candidate:false}];await navigate('/settings');await settingsReady();assert.equal(await evaluate(`document.querySelector('#output_device').hidden`),false);
 // Invalid values in a different category reveal the field rather than failing silently.
 await evaluate(`document.querySelector('#warmup_ms').value='1';document.querySelector('#warmup_ms').dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('#save-settings').click()`,true);
 await wait(`!document.querySelector('#advanced').hidden&&document.querySelector('#warmup_ms').closest('details').open`);
 await evaluate(`document.querySelector('#discard-settings').click()`,true);await wait(`document.querySelector('#settings-footer').hidden`);

 await navigate('/phone');await wait(`document.querySelector('#talk')!==null`);assert.equal(await evaluate(`getComputedStyle(document.body).backgroundColor`),'rgb(22, 24, 35)');assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true);
 assert.equal(state.unauthorized,0,'shared credentials reused');assert.deepEqual(errors,[]);
 console.log(platform+': PASS: 1280x720 home pass/fail/expired/paired without scrolling; QR reused across Settings; network 0/1/many; save/dirty/restart/locked controls; device rename/revoke; category navigation/dirty/discard/hidden-field validation; consistent typography; help anchors; small viewport; unchanged phone theme.');
}catch(error){console.error(error);throw error}
finally{
 if(cdp?.readyState===WebSocket.OPEN){cdp.send(JSON.stringify({id:999999,method:'Browser.close'}));await sleep(200)}
 cdp?.close();if(browser.exitCode===null)browser.kill();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await rm(dir,{recursive:true,force:true,maxRetries:5,retryDelay:200}).catch(()=>{});
}
