export const el=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
const incoming=new URLSearchParams(location.hash.slice(1)).get('token');
export const adminToken=incoming||sessionStorage.getItem('doubao-microphone-admin-token')||'';
if(incoming){sessionStorage.setItem('doubao-microphone-admin-token',incoming);history.replaceState(null,'',location.pathname+location.search)}
export async function api(path:string,method='GET',body?:unknown,signal?:AbortSignal):Promise<any>{
  const response=await fetch('/api/'+path,{method,headers:{Authorization:'Bearer '+adminToken,...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body),cache:'no-store',signal});
  const data=await response.json();if(!response.ok)throw Error(data.error||'请求失败');return data;
}
export function showDetails(title:string,detail:string){
  el('detail-title').textContent=title;el('detail-text').textContent=detail;
  const dialog=el<HTMLDialogElement>('detail-dialog');if(!dialog.open)dialog.showModal();
}
let lastError='';
export function showError(error:unknown,summary='操作未完成，请查看详情。'){
  lastError=error instanceof Error?error.message:String(error);el('error-summary').textContent=summary;el('error').hidden=false;
}
export function notice(message:string){el('notice').textContent=message;el('error').hidden=true}
export function runtimeLabel(status:any):string{
  return status.paused?'已暂停':status.input_state==='recovering'?'正在恢复':status.input_state==='failed'||status.error?'需要处理':status.testing?'正在测试':status.connected?'手机已连接':'已就绪';
}
export function deviceName(device:{id:string;label:string}):string{
  const name=device.label==='iPhone App'?'iPhone':device.label==='Android App'?'Android 手机':device.label;
  return name+' · '+device.id.replaceAll('-','').slice(0,8).toUpperCase();
}
el('error-details').addEventListener('click',()=>showDetails('详细原因',lastError));
el('detail-close').addEventListener('click',()=>el<HTMLDialogElement>('detail-dialog').close());
