import './style.css';
import { frame, congested } from './framing';

const el=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
const talk=el<HTMLButtonElement>('talk');
const waveform=el('voice-waveform'),waveBars=[...waveform.querySelectorAll<HTMLElement>('span')];
const waveSamples=Array<number>(waveBars.length).fill(0);
let waveActive=false;
function updateWaveform(level:number){
  const value=waveActive&&Number.isFinite(level)?Math.min(1,Math.max(0,level)):0;
  waveSamples.shift();waveSamples.push(value);
  waveform.setAttribute('aria-valuenow',String(Math.round(value*100)));
  waveBars.forEach((bar,index)=>{
    const envelope=Math.sin((index+1)/(waveBars.length+1)*Math.PI)**0.7;
    const sample=waveSamples[waveSamples.length-1-Math.abs(index-Math.floor(waveBars.length/2))];
    const amplitude=Math.sqrt(Math.min(1,Math.max(0,sample-0.008)*4));
    bar.style.height=(6+amplitude*envelope*128)+'px';
  });
}
function setWaveActive(active:boolean){
  if(waveActive===active)return;waveActive=active;waveform.classList.toggle('is-active',active);
  waveSamples.fill(0);updateWaveform(0);
}
let token=localStorage.getItem('ptt-device-token')||'';
const pairTicket=new URLSearchParams(location.hash.slice(1)).get('pair');
history.replaceState(null,'',location.pathname);
let ws:WebSocket|null=null,ctx:AudioContext|null=null,media:MediaStream|null=null,worklet:AudioWorkletNode|null=null;
let heartbeat:ReturnType<typeof setInterval>|undefined,connected=false,connecting=false;
let generation=0,needsGesture=false;
let connectTask:Promise<boolean>|null=null;
let audioWait:((ready:boolean)=>void)|null=null,socketWait:((ready:boolean)=>void)|null=null;
type Hold={pointerId:number;top:number;above:boolean};
let hold:Hold|null=null;
let session:{id:string;ending:boolean;ready:boolean;pointerId:number;top:number;above:boolean;action:'finish'|'enter'}|null=null;
let releaseEnter=false,pendingAction:string|null=null;
let endTimer:ReturnType<typeof setTimeout>|undefined;
const status=(s:string)=>{el('status').textContent=s};
const message=(type:string,values:Record<string,unknown>={})=>{if(ws?.readyState!==WebSocket.OPEN)throw Error('电脑连接已断开');ws.send(JSON.stringify({type,...values}));};
function render(){
  setWaveActive(!!hold||!!session&&!session.ending);
  talk.disabled=!token||!!session?.ending;talk.setAttribute('aria-pressed',String(!!session||!!hold));
  el('badge').textContent=token?'已配对':'未配对';
  el('talk-label').textContent=session?.ending?'正在结束':session?.ready?'正在输入':session?'准备中':hold?'正在连接':'按住说话';
  el('talk-hint').textContent=session?.ending?'正在传完尾音':!connected?(needsGesture?'按住即可启用麦克风':connecting?'正在连接麦克风':'按住即可连接麦克风'):releaseEnter?'松开后按 Enter':'松开结束本次输入';
  talk.setAttribute('aria-label',releaseEnter?'按住说话，松开后按 Enter':'按住说话，松开结束');
  el('pair-section').hidden=!pairTicket||!!token;
}
function disconnect(reason:string){
  generation++;hold=null;needsGesture=false;
  audioWait?.(false);audioWait=null;socketWait?.(false);socketWait=null;connectTask=null;
  clearInterval(heartbeat);clearTimeout(endTimer);heartbeat=undefined;
  if(ws?.readyState===WebSocket.OPEN){try{if(session)message('cancel',{session_id:session.id});if(pendingAction)message('cancel',{session_id:pendingAction});}catch{}}
  pendingAction=null;releaseEnter=false;
  session=null;connected=false;connecting=false;
  const old=ws;ws=null;old?.close();worklet?.port.postMessage({type:'cancel'});worklet?.disconnect();worklet=null;
  media?.getTracks().forEach(t=>t.stop());media=null;const oldCtx=ctx;ctx=null;void oldCtx?.close();
  status(reason);render();
}
function cancel(reason:string){
  const wasHolding=!!hold;hold=null;
  if(!session&&!pendingAction){if(wasHolding){status(reason);render();}return;}try{if(session)message('cancel',{session_id:session.id});if(pendingAction)message('cancel',{session_id:pendingAction});}catch{}
  pendingAction=null;
  worklet?.port.postMessage({type:'cancel'});session=null;clearTimeout(endTimer);status(reason);render();
}

function connectMicrophone():Promise<boolean>{
  if(ctx&&ctx.state!=='closed')void ctx.resume().catch(()=>{});
  if(connected)return Promise.resolve(true);
  if(connectTask)return connectTask;
  if(!token||document.hidden)return Promise.resolve(false);
  connecting=true;needsGesture=false;render();status('正在连接麦克风…');
  const attempt=++generation;
  const task=initializeMicrophone(attempt);connectTask=task;
  void task.then(()=>{if(connectTask===task)connectTask=null;});
  return task;
}
async function initializeMicrophone(attempt:number):Promise<boolean>{
  try{
    if(!window.isSecureContext||!navigator.mediaDevices?.getUserMedia)throw Error('麦克风需要可信 HTTPS。请先安装并信任本机证书。');
    const audioContext=new AudioContext({latencyHint:'interactive'});ctx=audioContext;
    // Called only by a trusted press; permission and audio startup run together.
    void audioContext.resume().catch(()=>{});
    const capture=await navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:true,noiseSuppression:true,autoGainControl:true},video:false});
    if(attempt!==generation){capture.getTracks().forEach(t=>t.stop());void audioContext.close();return false;}media=capture;
    for(const track of capture.getTracks()){track.addEventListener('ended',()=>{if(attempt===generation)disconnect('麦克风被系统中断，请重新连接。')});track.addEventListener('mute',()=>{if(attempt===generation)disconnect('麦克风被其他应用中断，请重新连接。')});}
    audioContext.addEventListener('statechange',()=>{
      if(attempt!==generation)return;
      if(audioContext.state==='running'){
        needsGesture=false;audioWait?.(true);audioWait=null;render();
      }else if(audioContext.state==='closed'||connected){disconnect('系统暂停了麦克风，按住说话可重新连接。');}
    });
    void audioContext.resume().catch(()=>{});
    if(audioContext.state!=='running'){
      needsGesture=true;status('按住说话即可启用麦克风。');render();
      const resumed=await new Promise<boolean>(resolve=>{audioWait=resolve;});
      if(!resumed||attempt!==generation)return false;
    }
    await audioContext.audioWorklet.addModule('/audio-worklet.js');
    if(attempt!==generation||!ctx)return false;
    worklet=new AudioWorkletNode(ctx,'ptt-capture',{numberOfInputs:1,numberOfOutputs:1,outputChannelCount:[1]});
    ctx.createMediaStreamSource(media).connect(worklet);worklet.connect(ctx.destination);
    worklet.port.onmessage=({data})=>{
      if(data.type==='meter'){
        updateWaveform(data.level*5);return;
      }
      if(!session||data.id!==session.id)return;
      try{
        if(data.type==='audio'){
          if(!ctx||!ws||ws.readyState!==WebSocket.OPEN)throw Error('音频连接已断开');
          if(congested(ws.bufferedAmount,ctx.sampleRate))throw Error('网络积压过多，本次已停止。请检查 Wi-Fi 后重试。');
          ws.send(frame(session.id,data.sequence,data.samples));
        }else if(data.type==='end'){message('end',{session_id:session.id,last_seq:data.last_seq,...(releaseEnter?{action:session.action}:{})});}
      }catch(e){disconnect(String(e));}
    };
    ws=new WebSocket(`wss://${location.host}/ws`);const socket=ws;
    const ready=new Promise<boolean>(resolve=>{socketWait=resolve;});
    const handshake=setTimeout(()=>{if(attempt===generation&&!connected)disconnect('连接超时，请检查电脑程序和防火墙。')},7000);
    socket.addEventListener('open',()=>{if(attempt!==generation)return;try{message('hello',{version:1,token,sample_rate:ctx!.sampleRate,channels:1})}catch(e){disconnect(String(e))}});
    socket.addEventListener('message',({data})=>{
      if(attempt!==generation)return;
      const m=JSON.parse(data);
      if(m.type==='hello'){
        if(audioContext.state!=='running'){disconnect('麦克风尚未就绪，按住说话可重新连接。');return;}
        clearTimeout(handshake);connecting=false;connected=true;needsGesture=false;socketWait?.(true);socketWait=null;releaseEnter=Array.isArray(m.capabilities)&&m.capabilities.includes('release_enter');el('mock').hidden=!m.mock;
        heartbeat=setInterval(()=>{try{message('heartbeat')}catch{disconnect('连接已断开，请重新连接。')}},250);
        status(hold?'正在准备输入。':'按住说话即可开始。');render();
      }else if(m.type==='ready'&&session&&m.session_id===session.id){session.ready=true;status('正在向电脑输入声音，松开结束。');render();}
      else if(m.type==='ended'&&m.session_id===session?.id){pendingAction=m.enter_pending?m.session_id:null;session=null;clearTimeout(endTimer);status(m.complete?(m.enter_pending?'听写已结束，等待按 Enter。':'本次声音已传完，电脑快捷键已释放。'):m.reason==='short_press'?'按住时间太短，本次已取消。':m.reason==='doubao_stopped'?'豆包提前结束了录音，本次已停止，请重新按住。':`本次已停止（${m.reason}），请重新按住。`);render();}
      else if(m.type==='action_result'&&m.session_id===pendingAction&&!session){pendingAction=null;status(m.status==='executed'?'已按 Enter。':m.status==='failed'?'Enter 未完成，请在电脑操作。':'本次未按 Enter。');}
      else if(m.type==='error'){if(m.code==='auth'){token='';localStorage.removeItem('ptt-device-token');}disconnect(m.message);}
    });
    socket.addEventListener('close',()=>{clearTimeout(handshake);if(ws===socket)disconnect('与电脑断开，按住说话可重新连接。')});
    socket.addEventListener('error',()=>{clearTimeout(handshake);if(ws===socket)disconnect('无法连接电脑，请检查证书信任、Wi-Fi 和防火墙。')});
    return await ready;
  }catch(e){if(attempt===generation)disconnect(e instanceof Error?e.message:String(e));return false;}
}

function startHeldSpeech(pressed:Hold){
  if(hold!==pressed||!connected||session||document.hidden)return;
  const id=crypto.randomUUID();session={id,ending:false,ready:false,...pressed,action:'finish'};
  try{pendingAction=null;message('start',{session_id:id});worklet!.port.postMessage({type:'start',id});status('正在启动电脑语音输入…');render();}catch(err){disconnect(String(err));}
}
talk.addEventListener('pointerdown',e=>{
  if(!token||session||hold||e.button!==0||!e.isPrimary)return;e.preventDefault();
  const pressed:Hold={pointerId:e.pointerId,top:talk.getBoundingClientRect().top,above:false};hold=pressed;
  try{talk.setPointerCapture(e.pointerId);}catch{hold=null;return;}
  if(connected){startHeldSpeech(pressed);return;}
  // This call resumes AudioContext synchronously within the trusted pointer event.
  void connectMicrophone().then(ready=>{if(ready)startHeldSpeech(pressed);});render();
});
talk.addEventListener('pointermove',e=>{
  if(hold&&e.pointerId===hold.pointerId){e.preventDefault();hold.above=e.clientY<hold.top;}
  if(session&&!session.ending&&e.pointerId===session.pointerId)session.above=e.clientY<session.top;
});
talk.addEventListener('pointerup',e=>{
  if(!hold||e.pointerId!==hold.pointerId)return;e.preventDefault();hold=null;
  if(!session||session.ending||e.pointerId!==session.pointerId){status(connected?'再次按住说话。':connecting?'正在连接麦克风…':'按住说话可重新连接。');render();return;}
  session.above=e.clientY<session.top;
  session.action=releaseEnter&&!session.above?'enter':'finish';
  session.ending=true;worklet?.port.postMessage({type:'end',id:session.id});render();
  endTimer=setTimeout(()=>disconnect('结束确认超时，已断开以释放电脑快捷键。'),1500);
});
talk.addEventListener('pointercancel',e=>{if(hold?.pointerId===e.pointerId||session?.pointerId===e.pointerId)cancel('触摸已取消，本次输入已停止。')});
talk.addEventListener('lostpointercapture',e=>{if(hold?.pointerId===e.pointerId||(session&&!session.ending&&e.pointerId===session.pointerId))cancel('触摸已中断，请重新按住。')});
talk.addEventListener('contextmenu',e=>e.preventDefault());
document.addEventListener('visibilitychange',()=>{if(document.hidden)disconnect('页面已离开前台，本次输入已停止。')});
window.addEventListener('pagehide',()=>disconnect('页面已关闭。'));
window.addEventListener('blur',()=>{if(hold||session||pendingAction)cancel('页面失去焦点，本次输入已停止。')});

el('pair').addEventListener('click',async()=>{
  const button=el<HTMLButtonElement>('pair');button.disabled=true;
  try{
    const r=await fetch('/pair',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({ticket:pairTicket,label:el<HTMLInputElement>('label').value})});const p=await r.json();if(!r.ok)throw Error(p.error);
    el('pair-status').textContent='正在完成配对…';
    const started=Date.now();
    while(Date.now()-started<300000){
      await new Promise(r=>setTimeout(r,1000));const r=await fetch(`/pair-result/${p.id}?poll=${encodeURIComponent(p.poll)}`,{cache:'no-store'});const result=await r.json();if(!r.ok)throw Error(result.error);
      if(result.token){token=result.token;localStorage.setItem('ptt-device-token',token);status('已配对，按住说话即可开始。');render();return;}
    }throw Error('配对超时，请在电脑重新生成二维码。');
  }catch(e){el('pair-status').textContent=String(e);button.disabled=false;}
});
void fetch('/info').then(r=>r.json()).then(i=>{el('computer').textContent=i.computer_name;el('mock').hidden=!i.mock}).catch(()=>status('无法读取电脑信息。'));
if(token)status('按住说话即可开始。');
render();
