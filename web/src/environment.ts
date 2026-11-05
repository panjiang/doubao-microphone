type State='loading'|'pass'|'fail'|'unknown'|'not_applicable';
type Check={id:string;title?:string;state:Exclude<State,'loading'>;detail:string};
type Report={supported:boolean;checks:Check[]};
const items=[
  ['virtual_microphone','虚拟麦克风'],['doubao_installation','豆包安装及运行'],
  ['doubao_microphone','豆包麦克风配置'],['doubao_trigger','长按快捷键配置'],
  ['doubao_voice_switch','语音快捷键开关'],
] as const;
const labels:Record<State,string>={loading:'检测中',pass:'通过',fail:'失败',unknown:'无法确认',not_applicable:'不适用'};
const marks={
  pass:'<path d="m7 12 3 3 7-7"/>',
  fail:'<path d="m8 8 8 8m0-8-8 8"/>',
  unknown:'<path d="M9.5 9a2.5 2.5 0 1 1 3.4 2.34c-.9.36-.9 1.16-.9 1.66m0 3v.1"/>',
  not_applicable:'<path d="M8 12h8"/>',
};

export function initEnvironment(request:(signal:AbortSignal)=>Promise<Report>):void {
  const list=document.getElementById('environment-checks')!;
  const button=document.getElementById('environment-refresh') as HTMLButtonElement;
  const legacy=document.getElementById('environment-legacy')!;
  const note=document.getElementById('environment-note')!;
  let running=false;
  let displayItems:ReadonlyArray<readonly [string,string]>=items;

  function row(id:string,title:string,state:State,detail:string):HTMLLIElement {
    const row=document.createElement('li');row.className='environment-row';row.dataset.check=id;row.dataset.state=state;
    const icon=document.createElement('span');icon.className='environment-icon '+(state==='loading'?'is-loading':state);icon.setAttribute('aria-hidden','true');
    if(state!=='loading')icon.innerHTML=`<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="11" fill="currentColor"/><g fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${marks[state]}</g></svg>`;
    const body=document.createElement('div');body.className='environment-body';
    const heading=document.createElement('div');heading.className='environment-row-title';
    const name=document.createElement('strong');name.textContent=title;
    const badge=document.createElement('span');badge.className='environment-state '+state;badge.textContent=labels[state];
    heading.append(name,badge);
    const description=document.createElement('p');description.textContent=detail;
    body.append(heading,description);row.append(icon,body);return row;
  }
  async function refresh():Promise<void> {
    if(running)return;running=true;button.disabled=true;
    list.setAttribute('aria-busy','true');
    list.replaceChildren(...displayItems.map(([id,title])=>row(id,title,'loading','正在检查…')));
    const controller=new AbortController();let timedOut=false;
    const timer=setTimeout(()=>{timedOut=true;controller.abort()},10000);
    try {
      const report=await request(controller.signal);
      if(typeof report?.supported!=='boolean'||!Array.isArray(report.checks))throw Error('检测响应无法识别');
      if(!report.supported){list.hidden=true;button.hidden=true;note.hidden=true;legacy.hidden=false;return;}
      legacy.hidden=true;list.hidden=false;note.hidden=false;button.hidden=false;
      const extra=report.checks.filter(item=>!items.some(([id])=>id===item.id)).map(item=>[item.id,item.title||item.id] as const);
      displayItems=[...items,...extra];
      list.replaceChildren(...displayItems.map(([id,title])=>{
        const result=report.checks.find(item=>item.id===id);
        if(!result||!['pass','fail','unknown','not_applicable'].includes(result.state)||typeof result.detail!=='string')return row(id,title,'unknown','未取得有效检测结果，请重新检测。');
        return row(id,typeof result.title==='string'?result.title:title,result.state,result.detail);
      }));
    } catch(e) {
      const detail=timedOut?'检测超过 10 秒，请检查程序是否运行后重新检测。':`检测未完成：${e instanceof Error?e.message:String(e)}。请重新检测。`;
      list.replaceChildren(...displayItems.map(([id,title])=>row(id,title,'unknown',detail)));
    } finally {
      clearTimeout(timer);list.setAttribute('aria-busy','false');button.disabled=false;running=false;
    }
  }
  button.addEventListener('click',()=>void refresh());
  void refresh();
}
