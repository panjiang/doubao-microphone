// Optional real Chromium/Edge UI smoke test. Uses a fresh browser profile and
// synthetic microphone; never accesses the person's browser profile or microphone.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,writeFile,mkdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import net from 'node:net';
import WebSocket from '../web/node_modules/ws/wrapper.mjs';

const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
const dir=await mkdtemp(join(tmpdir(),'ptt-browser-'));const admin=await port(),mobile=await port(),bootstrap=await port();
await writeFile(join(dir,'settings.json'),JSON.stringify({computer_name:'豆包麦克风 · 浏览器测试',lan_ip:'127.0.0.1',admin_port:admin,mobile_port:mobile,bootstrap_port:bootstrap,output_device:'mock',shortcut:'left_alt',warmup_ms:350,autostart:false}));
const binary=process.env.PTT_BINARY||resolve('target/debug/doubao-microphone'+(process.platform==='win32'?'.exe':''));
const edge=process.env.EDGE_PATH||(process.platform==='win32'?'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe':'chromium');
const bridge=spawn(binary,['--mock','--data-dir',dir,'serve',...(process.platform==='win32'?[]:['--headless'])],{stdio:['ignore','ignore','pipe']});
let diagnostics='';bridge.stderr.on('data',b=>diagnostics=(diagnostics+b).slice(-4000));
let browser,cdp;let token;let debugPage;const errors=[];
const artifacts=resolve('artifacts');await mkdir(artifacts,{recursive:true});
async function api(path,method='GET',body){const r=await fetch(`http://127.0.0.1:${admin}/api/${path}`,{method,headers:{Authorization:'Bearer '+token,...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify(body):undefined});const v=await r.json();if(!r.ok)throw Error(v.error);return v;}
try {
 for(let n=0;n<200;n++){if(bridge.exitCode!==null)throw Error(diagnostics);try{token=(await readFile(join(dir,'admin-url.txt'),'utf8')).split('#token=')[1];break}catch{await sleep(30)}}
 assert(token,'bridge startup');
 const profile=join(dir,'browser');await mkdir(profile);
 browser=spawn(edge,['--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check','--remote-debugging-port=0','--remote-allow-origins=*',`--user-data-dir=${profile}`,'--ignore-certificate-errors','--use-fake-ui-for-media-stream','--use-fake-device-for-media-stream','about:blank'],{stdio:'ignore'});
 let wsUrl;
 for(let n=0;n<200;n++){try{const [port,path]=(await readFile(join(profile,'DevToolsActivePort'),'utf8')).trim().split('\n');wsUrl=`ws://127.0.0.1:${port}${path}`;break}catch{await sleep(50)}}
 assert(wsUrl,'headless browser startup');cdp=new WebSocket(wsUrl);await new Promise((r,j)=>{cdp.once('open',r);cdp.once('error',j)});
 let next=1;const pending=new Map();
 cdp.on('message',b=>{const m=JSON.parse(b.toString());if(m.id){const p=pending.get(m.id);if(p){pending.delete(m.id);clearTimeout(p.timer);m.error?p.reject(Error(m.error.message)):p.resolve(m.result)}}else if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails.text)});
 function call(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=next++;const timer=setTimeout(()=>{pending.delete(id);reject(Error('CDP timeout: '+method))},10000);pending.set(id,{resolve,reject,timer});cdp.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}))})}
 const {targetId}=await call('Target.createTarget',{url:'about:blank'});const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
 const command=(method,params={})=>call(method,params,sessionId);
 const evaluate=async(expression,userGesture=false)=>{const r=await command('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true,userGesture});if(r.exceptionDetails)throw Error(r.exceptionDetails.exception?.description||r.exceptionDetails.text);return r.result.value};
 debugPage=()=>evaluate('document.body.innerText');
 await command('Runtime.enable');await command('Page.enable');
 await command('Emulation.setDeviceMetricsOverride',{width:1200,height:1100,deviceScaleFactor:1,mobile:false});
 await command('Page.navigate',{url:`http://127.0.0.1:${admin}/#token=${token}`});
 for(let n=0;n<100;n++){if(await evaluate('document.querySelectorAll("#output_device option").length>0'))break;await sleep(50)}
 assert(await evaluate('document.querySelectorAll("#output_device option").length>0'),'admin settings loaded');
 assert.equal(await evaluate('document.querySelector("#error").hidden'),true,'admin error banner');
 assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true,'desktop overflow');
 const adminShot=await command('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});await writeFile(join(artifacts,'admin-desktop.png'),Buffer.from(adminShot.data,'base64'));
 const ticket=await api('pair-ticket','POST');
 await command('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
 await command('Emulation.setTouchEmulationEnabled',{enabled:true,maxTouchPoints:1});
 await command('Page.navigate',{url:ticket.phone_url});
 for(let n=0;n<100;n++){if(await evaluate('document.querySelector("#computer")?.textContent.includes("浏览器测试")'))break;await sleep(50)}
 await evaluate('document.querySelector("#pair").click()',true);
 let request;
 for(let n=0;n<100;n++){request=(await api('status')).pending;if(request)break;await sleep(50)}
 assert(request,'phone pairing request');await api('pair-approve/'+request.id,'POST');
 for(let n=0;n<100;n++){if(await evaluate('!document.querySelector("#connect").disabled'))break;await sleep(50)}
 await evaluate('document.querySelector("#connect").click()',true);
 for(let n=0;n<100;n++){if((await api('status')).status.connected)break;await sleep(50)}
 assert((await api('status')).status.connected,'browser fake microphone connected');
 const rect=await evaluate('(()=>{const r=document.querySelector("#talk").getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()');
 await command('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:rect.x,y:rect.y}]});await sleep(600);
 assert.equal((await api('status')).status.phase,'streaming','touch and real AudioWorklet pipeline');
 assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true,'phone overflow');
 const phoneShot=await command('Page.captureScreenshot',{format:'png'});await writeFile(join(artifacts,'phone-recording.png'),Buffer.from(phoneShot.data,'base64'));
 await command('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await sleep(850);
 const state=(await api('status')).status;assert.equal(state.session_id,null);assert.equal(state.phase,'idle');assert.deepEqual(errors,[]);
 await evaluate('document.querySelector("#disconnect").click()',true);await sleep(100);assert.equal((await api('status')).status.connected,false);
 console.log('PASS: real Edge/Chromium admin + mobile viewport, pairing, synthetic microphone/AudioWorklet, touch hold/release, no horizontal overflow or JS exceptions. iPhone Safari and real dictation NOT tested.');
}catch(e){console.error(diagnostics);if(debugPage)console.error(await debugPage().catch(()=>''));console.error('Browser exceptions:',errors);throw e}
finally {
 if(cdp?.readyState===WebSocket.OPEN){cdp.send(JSON.stringify({id:999999,method:'Browser.close'}));await sleep(300);}if(cdp)cdp.close();if(browser&&browser.exitCode===null){browser.kill();await sleep(200)}bridge.kill();await new Promise(r=>bridge.exitCode!==null?r():bridge.once('exit',r));
 await sleep(200);await rm(dir,{recursive:true,force:true,maxRetries:5,retryDelay:200}).catch(()=>{});
}
