import './style.css';
import { frame, congested } from './framing';

const el=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
const talk=el<HTMLButtonElement>('talk'),connect=el<HTMLButtonElement>('connect');
let token=localStorage.getItem('ptt-device-token')||'';
const pairTicket=new URLSearchParams(location.hash.slice(1)).get('pair');
history.replaceState(null,'',location.pathname);
let ws:WebSocket|null=null,ctx:AudioContext|null=null,media:MediaStream|null=null,worklet:AudioWorkletNode|null=null;
let heartbeat:ReturnType<typeof setInterval>|undefined,connected=false,connecting=false;
let generation=0;
let session:{id:string;ending:boolean;ready:boolean;pointerId:number;top:number;above:boolean;action:'finish'|'enter'}|null=null;
let releaseEnter=false,pendingAction:string|null=null;
let endTimer:ReturnType<typeof setTimeout>|undefined;
const status=(s:string)=>{el('status').textContent=s};
const message=(type:string,values:Record<string,unknown>={})=>{if(ws?.readyState!==WebSocket.OPEN)throw Error('电脑连接已断开');ws.send(JSON.stringify({type,...values}));};
function render(){
  connect.disabled=connecting||connected||!token;
  connect.textContent=connecting?'正在连接…':connected?'麦克风已就绪':'连接麦克风';
  talk.disabled=!connected||!!session?.ending;talk.setAttribute('aria-pressed',String(!!session));
  el('badge').textContent=connected?'已连接':connecting?'连接中':'未连接';
  el('talk-label').textContent=session?.ending?'正在结束':session?.ready?'正在输入':session?'准备中':'按住说话';
  el('talk-hint').textContent=session?.ending?'正在传完尾音':!connected?'先连接麦克风':releaseEnter?'松开后按 Enter':'松开结束本次输入';
  talk.setAttribute('aria-label',releaseEnter?'按住说话，松开后按 Enter':'按住说话，松开结束');
  el('disconnect').hidden=!connected&&!connecting;
  el('pair-section').hidden=!pairTicket||!!token;
}
function disconnect(reason:string){
  generation++;
  clearInterval(heartbeat);clearTimeout(endTimer);heartbeat=undefined;
  if(ws?.readyState===WebSocket.OPEN){try{if(session)message('cancel',{session_id:session.id});if(pendingAction)message('cancel',{session_id:pendingAction});}catch{}}
  pendingAction=null;releaseEnter=false;
  session=null;connected=false;connecting=false;
  const old=ws;ws=null;old?.close();worklet?.port.postMessage({type:'cancel'});worklet?.disconnect();worklet=null;
  media?.getTracks().forEach(t=>t.stop());media=null;const oldCtx=ctx;ctx=null;void oldCtx?.close();
  el('level').style.width='0%';status(reason);render();
}
function cancel(reason:string){
  if(!session&&!pendingAction)return;try{if(session)message('cancel',{session_id:session.id});if(pendingAction)message('cancel',{session_id:pendingAction});}catch{}
  pendingAction=null;
  worklet?.port.postMessage({type:'cancel'});session=null;clearTimeout(endTimer);status(reason);render();
}

connect.addEventListener('click',async()=>{
  if(connecting||connected)return;
  connecting=true;render();status('正在初始化麦克风…');
  const attempt=++generation;
  try{
    if(!window.isSecureContext||!navigator.mediaDevices?.getUserMedia)throw Error('麦克风需要可信 HTTPS。请先安装并信任本机证书。');
    const audioContext=new AudioContext({latencyHint:'interactive'});ctx=audioContext;await audioContext.resume();
    const capture=await navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:true,noiseSuppression:true,autoGainControl:true},video:false});
    if(attempt!==generation){capture.getTracks().forEach(t=>t.stop());void audioContext.close();return;}media=capture;
    for(const track of capture.getTracks()){track.addEventListener('ended',()=>{if(attempt===generation)disconnect('麦克风被系统中断，请重新连接。')});track.addEventListener('mute',()=>{if(attempt===generation)disconnect('麦克风被其他应用中断，请重新连接。')});}
    await audioContext.audioWorklet.addModule('/audio-worklet.js');
    if(attempt!==generation||!ctx)return;
    audioContext.addEventListener('statechange',()=>{if(attempt===generation&&connected&&audioContext.state!=='running')disconnect('系统暂停了麦克风，请重新连接。')});
    worklet=new AudioWorkletNode(ctx,'ptt-capture',{numberOfInputs:1,numberOfOutputs:1,outputChannelCount:[1]});
    ctx.createMediaStreamSource(media).connect(worklet);worklet.connect(ctx.destination);
    worklet.port.onmessage=({data})=>{
      if(data.type==='meter'){
        const n=Math.min(100,Math.round(data.level*240));el('level').style.width=n+'%';el('level').parentElement?.setAttribute('aria-valuenow',String(n));return;
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
    const handshake=setTimeout(()=>{if(!connected)disconnect('连接超时，请检查电脑程序和防火墙。')},7000);
    socket.addEventListener('open',()=>{if(attempt!==generation)return;try{message('hello',{version:1,token,sample_rate:ctx!.sampleRate,channels:1})}catch(e){disconnect(String(e))}});
    socket.addEventListener('message',({data})=>{
      if(attempt!==generation)return;
      const m=JSON.parse(data);
      if(m.type==='hello'){
        clearTimeout(handshake);connecting=false;connected=true;releaseEnter=Array.isArray(m.capabilities)&&m.capabilities.includes('release_enter');el('mock').hidden=!m.mock;
        heartbeat=setInterval(()=>{try{message('heartbeat')}catch{disconnect('连接已断开，请重新连接。')}},250);
        status('麦克风就绪。把电脑光标放到输入框，再按住说话。');render();
      }else if(m.type==='ready'&&session&&m.session_id===session.id){session.ready=true;status('正在向电脑输入声音，松开结束。');render();}
      else if(m.type==='ended'&&m.session_id===session?.id){pendingAction=m.enter_pending?m.session_id:null;session=null;clearTimeout(endTimer);status(m.complete?(m.enter_pending?'听写已结束，等待按 Enter。':'本次声音已传完，电脑快捷键已释放。'):m.reason==='short_press'?'按住时间太短，本次已取消。':m.reason==='doubao_stopped'?'豆包提前结束了录音，本次已停止，请重新按住。':`本次已停止（${m.reason}），请重新按住。`);render();}
      else if(m.type==='action_result'&&m.session_id===pendingAction&&!session){pendingAction=null;status(m.status==='executed'?'已按 Enter。':m.status==='failed'?'Enter 未完成，请在电脑操作。':'本次未按 Enter。');}
      else if(m.type==='error'){if(m.code==='auth'){token='';localStorage.removeItem('ptt-device-token');}disconnect(m.message);}
    });
    socket.addEventListener('close',()=>{clearTimeout(handshake);if(ws===socket)disconnect('与电脑断开。重新连接后，再按住开始新的一次。')});
    socket.addEventListener('error',()=>{clearTimeout(handshake);if(ws===socket)disconnect('无法连接电脑，请检查证书信任、Wi-Fi 和防火墙。')});
  }catch(e){if(attempt===generation)disconnect(e instanceof Error?e.message:String(e));}
});

talk.addEventListener('pointerdown',e=>{
  if(!connected||session||e.button!==0||!e.isPrimary)return;e.preventDefault();
  const id=crypto.randomUUID();
  session={id,ending:false,ready:false,pointerId:e.pointerId,top:talk.getBoundingClientRect().top,above:false,action:'finish'};
  try{talk.setPointerCapture(e.pointerId);pendingAction=null;message('start',{session_id:id});worklet!.port.postMessage({type:'start',id});status('正在启动电脑语音输入…');render();}catch(err){disconnect(String(err));}
});
talk.addEventListener('pointermove',e=>{
  if(session&&!session.ending&&e.pointerId===session.pointerId){e.preventDefault();session.above=e.clientY<session.top;}
});
talk.addEventListener('pointerup',e=>{
  if(!session||session.ending||e.pointerId!==session.pointerId)return;e.preventDefault();
  session.above=e.clientY<session.top;
  session.action=releaseEnter&&!session.above?'enter':'finish';
  session.ending=true;worklet?.port.postMessage({type:'end',id:session.id});render();
  endTimer=setTimeout(()=>disconnect('结束确认超时，已断开以释放电脑快捷键。'),1500);
});
talk.addEventListener('pointercancel',e=>{if(session&&e.pointerId===session.pointerId)cancel('触摸已取消，电脑快捷键已请求释放。')});
talk.addEventListener('lostpointercapture',e=>{if(session&&!session.ending&&e.pointerId===session.pointerId)cancel('触摸已中断，请重新按住。')});
talk.addEventListener('contextmenu',e=>e.preventDefault());
document.addEventListener('visibilitychange',()=>{if(document.hidden)disconnect('页面已离开前台，本次输入已停止。')});
window.addEventListener('pagehide',()=>disconnect('页面已关闭。'));
window.addEventListener('blur',()=>{if(session||pendingAction)cancel('页面失去焦点，本次输入已停止。')});
el('disconnect').addEventListener('click',()=>disconnect('麦克风已关闭。'));

el('pair').addEventListener('click',async()=>{
  const button=el<HTMLButtonElement>('pair');button.disabled=true;
  try{
    const r=await fetch('/pair',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({ticket:pairTicket,label:el<HTMLInputElement>('label').value})});const p=await r.json();if(!r.ok)throw Error(p.error);
    el('pair-status').textContent='请到电脑配置页确认这部手机。';
    const started=Date.now();
    while(Date.now()-started<300000){
      await new Promise(r=>setTimeout(r,1000));const r=await fetch(`/pair-result/${p.id}?poll=${encodeURIComponent(p.poll)}`,{cache:'no-store'});const result=await r.json();if(!r.ok)throw Error(result.error);
      if(result.token){token=result.token;localStorage.setItem('ptt-device-token',token);status('已配对，点击“连接麦克风”开始。');render();return;}
    }throw Error('配对超时，请在电脑重新生成二维码。');
  }catch(e){el('pair-status').textContent=String(e);button.disabled=false;}
});
void fetch('/info').then(r=>r.json()).then(i=>{el('computer').textContent=i.computer_name;el('mock').hidden=!i.mock}).catch(()=>status('无法读取电脑信息。'));
if(token)status('已记住这台电脑，点击连接麦克风。');
render();
