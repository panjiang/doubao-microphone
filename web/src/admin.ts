import './style.css';
import {initEnvironment} from './environment';
const el=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
let token=new URLSearchParams(location.hash.slice(1)).get('token')||sessionStorage.getItem('doubao-microphone-admin-token')||'';
history.replaceState(null,'',location.pathname);if(token)sessionStorage.setItem('doubao-microphone-admin-token',token);
let config:Record<string,any>,paused=false,active=false,busy=false;
let automaticShortcut:string|undefined;
let shortcutLocked=false;
let currentPlatform='';
let qrTicket:string|undefined,qrGeneration=0;
function clearPairCode(){qrTicket=undefined;el('qr').replaceChildren();el('setup-link').hidden=true;el('phone-link').hidden=true;}
async function api(path:string,method='GET',body?:unknown,signal?:AbortSignal):Promise<any>{const r=await fetch('/api/'+path,{method,headers:{Authorization:'Bearer '+token,...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body),cache:'no-store',signal});const data=await r.json();if(!r.ok)throw Error(data.error||'请求失败');return data;}
function error(e:unknown){el('error').textContent=String(e);el('error').hidden=false;}
function notice(s:string){el('notice').textContent=s;el('error').hidden=true;}
const names:Record<string,string>={doubao_voice:'豆包兼容模式（Windows 1.0.0.23）',hid_right_alt:'右 Alt（UMDF HID 测试版）',hid_right_control:'右 Ctrl（UMDF HID 测试版）',left_alt:'左 Alt',right_alt:'右 Alt',left_control:'左 Control',right_control:'右 Control',left_command:'左 Command',right_command:'右 Command',fn:'Fn / 地球仪'};
function options(id:string,items:{value:string;label:string}[],value:string){const s=el<HTMLSelectElement>(id);s.replaceChildren(...items.map(i=>{const o=document.createElement('option');o.value=i.value;o.textContent=i.label;return o}));s.value=value;}
function syncShortcut(){
  if(!automaticShortcut)return;
  const manual=!shortcutLocked&&el<HTMLInputElement>('shortcut_manual').checked,select=el<HTMLSelectElement>('shortcut');
  if(!manual)select.value=automaticShortcut;
  select.disabled=active||!manual;
  const label=select.value==='hid_right_alt'?'右 Alt（UMDF）':names[select.value]||select.value;
  el<HTMLInputElement>('trigger-summary').value=shortcutLocked?'固定：'+label:(manual?'手动：':'自动：')+label;
  el('trigger-help').textContent=manual?'手动方式仅用于诊断，请与豆包的长按快捷键保持一致。修改后需保存设置。':currentPlatform==='macos'?'默认使用 Fn / 地球仪，需允许豆包麦克风使用辅助功能。检测不会发送按键。':automaticShortcut==='hid_right_alt'?'已检测到可访问的 UMDF 组件，使用右 Alt。检测不会发送按键。':'未检测到可访问的 UMDF 组件，使用豆包兼容模式；需要开启豆包悬浮工具栏，当前仅适配默认位置安装的 Windows 1.0.0.23。';
  el('codex-compat-row').hidden=select.value!=='doubao_voice';
  el('codex-compat-help').hidden=select.value!=='doubao_voice';
}
async function load(){
  const [s,setup]=await Promise.all([api('config'),api('setup')]);config=s;
  currentPlatform=setup.platform.platform;
  el<HTMLInputElement>('computer_name').value=s.computer_name;el<HTMLInputElement>('warmup_ms').value=String(s.warmup_ms);el<HTMLInputElement>('enter_delay_ms').value=String(s.enter_delay_ms??1000);el('enter-delay-row').hidden=!setup.platform.release_enter;el<HTMLInputElement>('autostart').checked=s.autostart;el<HTMLInputElement>('diagnostic_recording').checked=s.diagnostic_recording;el<HTMLInputElement>('codex_empty_compatibility').checked=s.codex_empty_compatibility;el('codex-compat-row').hidden=currentPlatform!=='windows';el('codex-compat-help').hidden=currentPlatform!=='windows';
  const windows=setup.platform.platform==='windows';
  const mac=currentPlatform==='macos',expectedOutput=mac?'BlackHole 2ch':'CABLE Input';
  const cableOutputs=setup.outputs.filter((d:any)=>d.virtual_candidate&&(mac?d.name==='BlackHole 2ch':/^CABLE Input(?:\s*\(|$)/i.test(d.name.trim())));
  if(windows)el('audio-setup').textContent=cableOutputs.length?'已检测到虚拟麦克风。程序输出使用 CABLE Input，请在豆包中选择 CABLE Output。':'尚未检测到虚拟麦克风。请重新运行 DoubaoMicrophone 安装包，向导会自动安装；提示需要重启时，请先重启电脑。';
  const available=windows||mac?setup.outputs.filter((d:any)=>cableOutputs.includes(d)||d.id==='mock'):setup.outputs;
  const singleCable=(windows||mac)&&cableOutputs.length===1;
  const outputValue=(d:any)=>singleCable&&d===cableOutputs[0]?expectedOutput:d.id;
  const outputs=available.map((d:any)=>({value:outputValue(d),label:windows?d.name:(d.virtual_candidate?'推荐 · ':'')+d.name}));
  const selected=available.find((d:any)=>d.id===s.output_device||d.name===s.output_device||d.name.startsWith(s.output_device+' ('));
  let chosen=selected?outputValue(selected):undefined;
  if(!chosen&&(windows||mac))chosen=outputs[0]?.value;
  if(!chosen){const missing=windows||mac?expectedOutput:s.output_device;outputs.unshift({value:missing,label:missing+'（未找到，请运行安装包或刷新）'});chosen=missing;}
  options('output_device',outputs,chosen);
  const shortcutNames=setup.platform.platform==='macos'?{...names,left_alt:'左 Option',right_alt:'右 Option'}:names;
  const shortcuts=setup.platform.shortcuts.length?setup.platform.shortcuts:Object.keys(shortcutNames);options('shortcut',shortcuts.map((k:string)=>({value:k,label:shortcutNames[k]||k})),s.shortcut);
  automaticShortcut=setup.platform.automatic_shortcut??undefined;
  shortcutLocked=setup.platform.shortcut_locked===true;
  el('trigger-summary-row').hidden=!automaticShortcut;el('trigger-diagnostics').hidden=!automaticShortcut||shortcutLocked;
  if(automaticShortcut){el('shortcut-advanced-host').append(el('shortcut-row'));el<HTMLInputElement>('shortcut_manual').checked=!!s.shortcut_manual;syncShortcut();}
  const ips=setup.interfaces.map((i:any)=>({value:i.ip,label:`${i.name}${i.ethernet?'（以太网优先）':''} · ${i.ip}`}));if(!ips.some((i:any)=>i.value===s.lan_ip))ips.unshift({value:s.lan_ip,label:s.lan_ip});options('lan_ip',ips,s.lan_ip);
  el('platform').textContent=`系统：${setup.platform.platform}。${setup.platform.native_supported?'原生端可用，仍需豆包实测。':'当前仅可进行模拟测试。'}`;
  el('permission').textContent=setup.platform.accessibility?'辅助功能权限检查通过。':mac?'请在系统设置 → 隐私与安全性 → 辅助功能中允许豆包麦克风。':'需要在系统设置 → 隐私与安全性 → 辅助功能中授权。';
  el('fingerprint').textContent=setup.ca_sha256;await devices();
}
async function devices(){const list=await api('devices');el('devices').replaceChildren(...list.map((d:any)=>{const row=document.createElement('div'),label=document.createElement('span'),button=document.createElement('button');label.textContent=d.label;button.textContent='撤销配对';button.addEventListener('click',async()=>{try{await api('devices/'+d.id,'DELETE');await devices();notice('设备凭据已撤销。')}catch(e){error(e)}});row.append(label,button);return row}));}
el('settings').addEventListener('submit',async e=>{
  e.preventDefault();if(active||busy)return;busy=true;
  try{const s={...config,computer_name:el<HTMLInputElement>('computer_name').value,output_device:el<HTMLSelectElement>('output_device').value,shortcut:el<HTMLSelectElement>('shortcut').value,shortcut_manual:automaticShortcut?el<HTMLInputElement>('shortcut_manual').checked:!!config.shortcut_manual,warmup_ms:Number(el<HTMLInputElement>('warmup_ms').value),enter_delay_ms:Number(el<HTMLInputElement>('enter_delay_ms').value),lan_ip:el<HTMLSelectElement>('lan_ip').value,autostart:el<HTMLInputElement>('autostart').checked,diagnostic_recording:el<HTMLInputElement>('diagnostic_recording').checked,codex_empty_compatibility:el<HTMLInputElement>('codex_empty_compatibility').checked};const result=await api('config','PUT',s);s.shortcut=result.shortcut??s.shortcut;if(automaticShortcut&&!s.shortcut_manual)automaticShortcut=s.shortcut;config=s;syncShortcut();notice(result.restart_required?'设置已保存。网络地址变更，请退出并重新启动程序，然后重新扫码。':'设置已保存。手机请重新连接麦克风。');}catch(e){error(e)}finally{busy=false;}
});
el('shortcut_manual').addEventListener('change',syncShortcut);
el('shortcut').addEventListener('change',syncShortcut);
el('refresh').addEventListener('click',()=>void load().catch(error));
el('test-key').addEventListener('click',async()=>{try{await api('test-key','POST');notice('请在 3 秒内切换到测试输入框；随后按住已保存的快捷键 3 秒。是否被豆包接受需人工观察。')}catch(e){error(e)}});
el('ticket').addEventListener('click',async()=>{
  const generation=++qrGeneration,button=el<HTMLButtonElement>('ticket');
  clearPairCode();button.disabled=true;
  try{
    const t=await api('pair-ticket','POST');if(generation!==qrGeneration)return;
    qrTicket=t.ticket_id;el('qr').innerHTML=t.qr_svg;
    const link=el<HTMLAnchorElement>('setup-link');link.hidden=false;link.href=t.url;
    const phone=el<HTMLAnchorElement>('phone-link');phone.hidden=false;phone.href=t.phone_url;phone.textContent='网页已信任证书？直接打开输入页';
    notice('App 和手机相机均可扫描此码，配对成功后二维码自动收起。');
  }catch(e){if(generation===qrGeneration)error(e)}finally{button.disabled=false;}
});
el('pause').addEventListener('click',async()=>{try{await api('pause','PUT',{paused:!paused})}catch(e){error(e)}});
async function poll(){
  try{const r=await api('status'),s=r.status;paused=s.paused;active=!!s.session_id||s.testing;el('mock').hidden=!s.mock;el('badge').textContent=paused?'已手动暂停':s.input_state==='recovering'?'输入服务恢复中':s.input_state==='failed'?'输入服务需处理':s.testing?'快捷键测试中':s.connected?'手机已连接':'未连接';
    const inputState=s.input_state==='recovering'?` · 输入服务恢复中（已尝试 ${s.input_recovery_attempts}/3）`:s.input_state==='failed'?' · 输入服务连续恢复失败，请检查权限或重启程序':'';
    el('runtime').textContent=`状态：${s.phase} · 音频排队：${s.queued_ms}ms${inputState}${s.error?' · '+s.error:''}`;
    el('pause').textContent=paused?'恢复接收':'暂停接收';
    for(const input of el('settings').querySelectorAll<HTMLInputElement|HTMLSelectElement|HTMLButtonElement>('input,select,button'))input.disabled=active;
    syncShortcut();
    if(qrTicket&&r.paired_ticket===qrTicket){clearPairCode();notice('配对成功，手机已保存电脑连接信息。');void devices().catch(error);}
  }catch(e){error(e);el('badge').textContent='服务不可用';}
}
void load().then(poll).catch(error);setInterval(()=>void poll(),1000);
setInterval(()=>void devices().catch(error),3000);
initEnvironment(signal=>api('environment','GET',undefined,signal));
