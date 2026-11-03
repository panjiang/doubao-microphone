import './style.css';
const el=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
let token=new URLSearchParams(location.hash.slice(1)).get('token')||sessionStorage.getItem('ptt-admin-token')||'';
history.replaceState(null,'',location.pathname);if(token)sessionStorage.setItem('ptt-admin-token',token);
let config:Record<string,any>,pending:string|undefined,paused=false,active=false,busy=false;
async function api(path:string,method='GET',body?:unknown):Promise<any>{const r=await fetch('/api/'+path,{method,headers:{Authorization:'Bearer '+token,...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body),cache:'no-store'});const data=await r.json();if(!r.ok)throw Error(data.error||'请求失败');return data;}
function error(e:unknown){el('error').textContent=String(e);el('error').hidden=false;}
function notice(s:string){el('notice').textContent=s;el('error').hidden=true;}
const names:Record<string,string>={doubao_voice:'豆包兼容模式（Windows 1.0.0.23）',hid_right_alt:'右 Alt（UMDF HID 测试版）',hid_right_control:'右 Ctrl（UMDF HID 测试版）',left_alt:'左 Alt',right_alt:'右 Alt',left_control:'左 Control',right_control:'右 Control',left_command:'左 Command',right_command:'右 Command',fn:'Fn / 地球仪'};
function options(id:string,items:{value:string;label:string}[],value:string){const s=el<HTMLSelectElement>(id);s.replaceChildren(...items.map(i=>{const o=document.createElement('option');o.value=i.value;o.textContent=i.label;return o}));s.value=value;}
async function load(){
  const [s,setup]=await Promise.all([api('config'),api('setup')]);config=s;
  el<HTMLInputElement>('computer_name').value=s.computer_name;el<HTMLInputElement>('warmup_ms').value=String(s.warmup_ms);el<HTMLInputElement>('autostart').checked=s.autostart;el<HTMLInputElement>('diagnostic_recording').checked=s.diagnostic_recording;el<HTMLInputElement>('codex_empty_compatibility').checked=s.codex_empty_compatibility;el('codex-compat-row').hidden=setup.platform.platform!=='windows';
  const outputs=setup.outputs.map((d:any)=>({value:d.id,label:(d.virtual_candidate?'推荐 · ':'')+d.name}));
  let chosen=setup.outputs.find((d:any)=>d.id===s.output_device||d.name===s.output_device||d.name.startsWith(s.output_device+' ('))?.id;
  if(!chosen){outputs.unshift({value:s.output_device,label:s.output_device+'（未找到，请安装或刷新）'});chosen=s.output_device;}
  options('output_device',outputs,chosen);
  const shortcutNames=setup.platform.platform==='macos'?{...names,left_alt:'左 Option',right_alt:'右 Option'}:names;
  const shortcuts=setup.platform.shortcuts.length?setup.platform.shortcuts:Object.keys(shortcutNames);options('shortcut',shortcuts.map((k:string)=>({value:k,label:shortcutNames[k]||k})),s.shortcut);
  const ips=setup.interfaces.map((i:any)=>({value:i.ip,label:`${i.name} · ${i.ip}`}));if(!ips.some((i:any)=>i.value===s.lan_ip))ips.unshift({value:s.lan_ip,label:s.lan_ip});options('lan_ip',ips,s.lan_ip);
  el('platform').textContent=`系统：${setup.platform.platform}。${setup.platform.native_supported?'原生端可用，仍需豆包实测。':'当前仅可进行模拟测试。'}`;
  el('permission').textContent=setup.platform.accessibility?'辅助功能权限检查通过。':'需要在系统设置 → 隐私与安全性 → 辅助功能中授权主应用与输入代理。';
  el('fingerprint').textContent=setup.ca_sha256;await devices();
}
async function devices(){const list=await api('devices');el('devices').replaceChildren(...list.map((d:any)=>{const row=document.createElement('div'),label=document.createElement('span'),button=document.createElement('button');label.textContent=d.label;button.textContent='撤销配对';button.addEventListener('click',async()=>{try{await api('devices/'+d.id,'DELETE');await devices();notice('设备凭据已撤销。')}catch(e){error(e)}});row.append(label,button);return row}));}
el('settings').addEventListener('submit',async e=>{
  e.preventDefault();if(active||busy)return;busy=true;
  try{const s={...config,computer_name:el<HTMLInputElement>('computer_name').value,output_device:el<HTMLSelectElement>('output_device').value,shortcut:el<HTMLSelectElement>('shortcut').value,warmup_ms:Number(el<HTMLInputElement>('warmup_ms').value),lan_ip:el<HTMLSelectElement>('lan_ip').value,autostart:el<HTMLInputElement>('autostart').checked,diagnostic_recording:el<HTMLInputElement>('diagnostic_recording').checked,codex_empty_compatibility:el<HTMLInputElement>('codex_empty_compatibility').checked};const result=await api('config','PUT',s);config=s;notice(result.restart_required?'设置已保存。网络地址变更，请退出并重新启动程序，然后重新扫码。':'设置已保存。手机请重新连接麦克风。');}catch(e){error(e)}finally{busy=false;}
});
el('refresh').addEventListener('click',()=>void load().catch(error));
el('test-key').addEventListener('click',async()=>{try{await api('test-key','POST');notice('请在 3 秒内切换到测试输入框；随后按住已保存的快捷键 3 秒。是否被豆包接受需人工观察。')}catch(e){error(e)}});
el('ticket').addEventListener('click',async()=>{try{const t=await api('pair-ticket','POST');el('qr').innerHTML=t.qr_svg;const link=el<HTMLAnchorElement>('setup-link');link.href=t.url;link.textContent=t.url;const phone=el<HTMLAnchorElement>('phone-link');phone.href=t.phone_url;phone.textContent='已信任证书？直接打开手机输入页';notice('二维码有效期 5 分钟，生成新二维码会作废旧二维码。')}catch(e){error(e)}});
el('approve').addEventListener('click',async()=>{if(pending)try{await api('pair-approve/'+pending,'POST');notice('已确认手机，请在手机连接麦克风。');await devices()}catch(e){error(e)}});
el('pause').addEventListener('click',async()=>{try{await api('pause','PUT',{paused:!paused})}catch(e){error(e)}});
async function poll(){
  try{const r=await api('status'),s=r.status;paused=s.paused;active=!!s.session_id||s.testing;el('mock').hidden=!s.mock;el('badge').textContent=s.testing?'快捷键测试中':paused?'已暂停':s.connected?'手机已连接':'未连接';
    el('runtime').textContent=`状态：${s.phase} · 音频排队：${s.queued_ms}ms${s.error?' · '+s.error:''}`;
    el('pause').textContent=paused?'恢复接收':'暂停接收';
    for(const input of el('settings').querySelectorAll<HTMLInputElement|HTMLSelectElement|HTMLButtonElement>('input,select,button'))input.disabled=active;
    pending=r.pending?.id;el('pending').hidden=!pending;if(pending)el('pending-label').textContent=`${r.pending.label} 请求配对。确认是你的手机后再允许。`;
  }catch(e){error(e);el('badge').textContent='服务不可用';}
}
void load().then(poll).catch(error);setInterval(()=>void poll(),1000);
