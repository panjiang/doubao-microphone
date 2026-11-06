// Real process + trusted local TLS + browser protocol; hardware stays explicitly mocked.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {resolve,join} from 'node:path';
import https from 'node:https';
import net from 'node:net';
import {randomUUID} from 'node:crypto';
import {createInterface} from 'node:readline';
import WebSocket from '../web/node_modules/ws/wrapper.mjs';

const binary=process.env.PTT_BINARY||resolve('target/debug/doubao-remote'+(process.platform==='win32'?'.exe':''));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
const dir=await mkdtemp(join(tmpdir(),'doubao-remote-test-'));
const admin=await freePort(),mobile=await freePort(),bootstrap=await freePort();
const config={computer_name:'Integration Test',lan_ip:'127.0.0.1',admin_port:admin,mobile_port:mobile,bootstrap_port:bootstrap,output_device:'mock',shortcut:'left_alt',warmup_ms:350,autostart:false};
await writeFile(join(dir,'settings.json'),JSON.stringify(config));
const child=spawn(binary,['--mock','--data-dir',dir,'serve','--headless'],{stdio:['ignore','ignore','pipe']});
let diagnostics='';child.stderr.on('data',b=>diagnostics=(diagnostics+b).slice(-6000));
const sockets=[];
let token,ca;
async function request(path,method='GET',body,auth=true,origin){
 const r=await fetch(`http://127.0.0.1:${admin}${path}`,{method,headers:{...(auth?{Authorization:'Bearer '+token}:{}),...(origin?{Origin:origin}:{}),...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify(body):undefined});
 return {status:r.status,data:await r.json()};
}
async function secure(path,method='GET',body){
 return new Promise((accept,reject)=>{
  const payload=body?JSON.stringify(body):'';
  const r=https.request({hostname:'127.0.0.1',port:mobile,path,method,ca,headers:{Origin:`https://127.0.0.1:${mobile}`,'Content-Type':'application/json','Content-Length':Buffer.byteLength(payload)}},res=>{let data='';res.on('data',b=>data+=b);res.on('end',()=>accept({status:res.statusCode,data:JSON.parse(data)}));});r.on('error',reject);r.end(payload);
 });
}
async function socket(secret){
 const ws=new WebSocket(`wss://127.0.0.1:${mobile}/ws`,{ca,origin:`https://127.0.0.1:${mobile}`});sockets.push(ws);
 const queue=[];let waiter;ws.on('message',b=>{const m=JSON.parse(b.toString());if(waiter){const w=waiter;waiter=undefined;w(m)}else queue.push(m)});
 await new Promise((r,j)=>{ws.once('open',r);ws.once('error',j)});
 const next=()=>queue.length?Promise.resolve(queue.shift()):new Promise((r,j)=>{const t=setTimeout(()=>{waiter=undefined;j(Error('WebSocket message timeout'))},3000);waiter=m=>{clearTimeout(t);r(m)}});
 const send=m=>ws.send(JSON.stringify(m));send({type:'hello',version:1,token:secret,sample_rate:48000,channels:1});return {ws,next,send};
}
async function pair(){
 const ticket=(await request('/api/pair-ticket','POST')).data;
 const oneTime=new URL(ticket.url).searchParams.get('token');
 assert.equal((await fetch(`http://127.0.0.1:${bootstrap}/ca.mobileconfig?token=${oneTime}`)).status,200);
 const p=await secure('/pair','POST',{ticket:oneTime,label:'Integration iPhone'});assert.equal(p.status,200);
 assert.equal((await secure('/pair','POST',{ticket:oneTime,label:'Replay'})).status,400);
 const url=`/pair-result/${p.data.id}?poll=${p.data.poll}`;
 assert.equal((await secure(url)).data.token,null);
 assert.equal((await request('/api/pair-approve/'+p.data.id,'POST')).status,200);
 const secret=(await secure(url)).data.token;assert(secret);
 assert.equal((await secure(url)).status,400);
 assert(!(await readFile(join(dir,'devices.json'),'utf8')).includes(secret));return {secret,id:p.data.id};
}
async function agentTest(){
 const p=spawn(binary,['--mock','input-agent'],{stdio:['pipe','pipe','inherit']});const lines=createInterface({input:p.stdout});
 const send=op=>new Promise((r,j)=>{const t=setTimeout(()=>j(Error('agent reply timeout')),2000);lines.once('line',line=>{clearTimeout(t);r(JSON.parse(line))});p.stdin.write(JSON.stringify(op)+'\n')});
 try{
  assert.equal((await send({op:'press',key:'left_alt'})).ok,true);
  await sleep(1150);assert.equal((await send({op:'renew'})).ok,false);
  assert.equal((await send({op:'press',key:'left_alt'})).ok,true);
  const exit=new Promise(r=>p.once('exit',r));p.stdin.end();assert.equal(await exit,0);
 }finally{lines.close();if(p.exitCode===null)p.kill();}
}
try{
 for(let n=0;n<200;n++){if(child.exitCode!==null)throw Error('server exited: '+diagnostics);try{token=(await readFile(join(dir,'admin-url.txt'),'utf8')).split('#token=')[1];break}catch{await sleep(30)}}
 assert(token,'server startup timeout');ca=await readFile(join(dir,'ca.pem'));
 assert.equal((await request('/api/status','GET',undefined,false)).status,400);
 assert.equal((await request('/api/status','GET',undefined,true,'https://evil.example')).status,400);
 assert.equal((await fetch(`http://127.0.0.1:${bootstrap}/ca.mobileconfig?token=invalid`)).status,400);
 const paired=await pair();
 let rejected=await socket('invalid');assert.equal((await rejected.next()).code,'auth');
 const c=await socket(paired.secret);assert.equal((await c.next()).type,'hello');
 rejected=await socket(paired.secret);assert.equal((await rejected.next()).code,'connect');
 const id=randomUUID();c.send({type:'start',session_id:id});await sleep(30);
 assert.equal((await request('/api/config','PUT',config)).status,400);
 for(let seq=0;seq<20;seq++){
  const bytes=Buffer.alloc(24+1920);bytes.write('PPT1');Buffer.from(id.replaceAll('-',''),'hex').copy(bytes,4);bytes.writeUInt32LE(seq,20);c.ws.send(bytes);
  if(seq%8===0)c.send({type:'heartbeat'});await sleep(20);
 }
 assert.equal((await c.next()).type,'ready');c.send({type:'end',session_id:id,last_seq:19});
 const ended=await c.next();assert.equal(ended.type,'ended');assert.equal(ended.complete,true);
 const short=randomUUID();c.send({type:'start',session_id:short});c.send({type:'end',session_id:short,last_seq:null});assert.equal((await c.next()).reason,'short_press');
 c.ws.close();await sleep(100);assert.equal((await request('/api/status')).data.status.connected,false);
 const d=await socket(paired.secret);assert.equal((await d.next()).type,'hello');d.send({type:'start',session_id:randomUUID()});
 await sleep(1150);const status=(await request('/api/status')).data.status;assert.equal(status.session_id,null);assert.equal(status.connected,false);
 assert.equal((await request('/api/config','PUT',{...config,warmup_ms:450})).status,200);
 assert.equal(JSON.parse(await readFile(join(dir,'settings.json'),'utf8')).warmup_ms,450);
 const e=await socket(paired.secret);assert.equal((await e.next()).type,'hello');
 assert.equal((await request('/api/config','PUT',{...config,warmup_ms:350})).status,200);
 const f=await socket(paired.secret);assert.equal((await f.next()).type,'hello');
 await sleep(100);assert.equal((await request('/api/status')).data.status.connected,true,'stale socket cleanup must not disconnect its replacement');
 const revoking=randomUUID();f.send({type:'start',session_id:revoking});await sleep(30);
 assert.equal((await request('/api/devices/'+paired.id,'DELETE')).status,200);
 assert.equal((await f.next()).reason,'revoked');
 rejected=await socket(paired.secret);assert.equal((await rejected.next()).code,'auth');
 assert.equal((await request('/api/test-key','POST')).status,200);
 assert.equal((await request('/api/config','PUT',config)).status,400);
 await sleep(6200);assert.equal((await request('/api/status')).data.status.testing,false);
 await agentTest();
 console.log('PASS: trusted TLS, approval/single-use pairing, auth/origin rejection, single client, PCM session, configuration lock, short click, disconnect, heartbeat expiry, revocation, input-agent lease expiry/EOF. Hardware mode NOT tested.');
}catch(e){console.error(diagnostics);throw e}
finally{
 for(const s of sockets)s.terminate();child.kill();await new Promise(r=>child.exitCode!==null?r():child.once('exit',r));await sleep(100);await rm(dir,{recursive:true,force:true});
}
