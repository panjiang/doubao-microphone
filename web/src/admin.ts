import './style.css';
import {api,el,adminToken,showError,notice,runtimeLabel,deviceName} from './admin-shared';
import {initEnvironment} from './environment';
import {readPairing,saveTicket,savePaired,secondsLeft,pairingKey,type PairingState} from './pairing-cache';
let pairing:PairingState|null=readPairing(sessionStorage,adminToken),busy=false;
let status:any=null,devices:{id:string;label:string}[]=[],attention=false,lastRuntimeError='';
function showStatus(){
  if(!status)return;
  el('badge').textContent=attention&&!status.paused?'需要处理':runtimeLabel(status);
  el<HTMLButtonElement>('pause').disabled=false;
  el('pause').textContent=status.paused?'恢复接收':'暂停接收';
  el('mock').hidden=!status.mock;
  const current=devices.find(device=>device.id===status.device_id);
  el('device-current').textContent=status.connected?(current?deviceName(current):'手机已连接'):'暂无手机连接';
  el('device-count').textContent=`已配对 ${devices.length} 台设备`;
}
function renderPairing(){
  const frame=el('qr'),button=el<HTMLButtonElement>('ticket');button.disabled=busy;
  if(pairing?.state==='paired'){
    frame.innerHTML='<div class="pair-success"><span aria-hidden="true">✓</span><strong>配对成功</strong><p>现在可以在手机上按住说话</p></div>';
    el('pair-status').textContent='手机已保存这台电脑的连接信息';button.textContent='添加手机';button.hidden=false;return;
  }
  if(pairing?.state==='ticket'){
    const left=secondsLeft(pairing.ticket);
    if(left){
      if(frame.dataset.ticket!==pairing.ticket.ticket_id){frame.innerHTML=pairing.ticket.qr_svg;frame.dataset.ticket=pairing.ticket.ticket_id}
      el('pair-status').textContent=`有效期 ${Math.floor(left/60)}:${String(left%60).padStart(2,'0')}`;button.hidden=true;
    }else{
      frame.replaceChildren();delete frame.dataset.ticket;
      const text=document.createElement('span');text.textContent='二维码已过期';frame.append(text);
      el('pair-status').textContent='刷新后即可重新扫码';button.textContent='刷新二维码';button.hidden=false;
    }
    return;
  }
  frame.textContent=busy?'正在准备二维码…':'二维码暂不可用';delete frame.dataset.ticket;
  el('pair-status').textContent=busy?'正在生成':'请重试';button.textContent=busy?'正在生成':'重新生成';button.hidden=false;
}
async function generate(){
  if(busy)return;busy=true;pairing=null;sessionStorage.removeItem(pairingKey);renderPairing();
  try{pairing=saveTicket(sessionStorage,adminToken,await api('pair-ticket','POST'))}
  catch(error){showError(error,'二维码生成失败，请重试。')}
  finally{busy=false;renderPairing()}
}
async function refreshDevices(){devices=await api('devices');showStatus()}
let polling=false;
async function poll(){
  if(polling)return;polling=true;
  try{
    const result=await api('status');status=result.status;showStatus();
    if(pairing?.state==='ticket'&&result.paired_ticket===pairing.ticket.ticket_id){pairing=savePaired(sessionStorage,adminToken);delete el('qr').dataset.ticket;renderPairing();notice('配对成功');await refreshDevices()}
    if(status.error&&status.error!==lastRuntimeError)showError(Error(status.error),'运行遇到问题，请查看详情。');
    lastRuntimeError=status.error||'';
  }catch(error){el('badge').textContent='服务不可用';el<HTMLButtonElement>('pause').disabled=true;showError(error,'无法连接应用，请确认豆包麦克风正在运行。')}
  finally{polling=false}
}
el('ticket').addEventListener('click',()=>void generate());
el('pause').addEventListener('click',async()=>{if(!status)return;try{await api('pause','PUT',{paused:!status.paused});await poll()}catch(error){showError(error)}});
async function start(){
  try{
    await refreshDevices();
    if(pairing?.state==='paired'&&!devices.length){pairing=null;sessionStorage.removeItem(pairingKey)}
    renderPairing();await poll();if(!pairing)await generate();
  }catch(error){showError(error,'首页加载失败，请刷新重试。');renderPairing()}
}
initEnvironment(signal=>api('environment','GET',undefined,signal),needsAttention=>{attention=needsAttention;showStatus()});
void start();setInterval(()=>void poll(),1000);
setInterval(()=>void refreshDevices().catch(error=>showError(error,'设备列表暂时无法读取。')),3000);
setInterval(()=>{if(pairing?.state==='ticket')renderPairing()},1000);
