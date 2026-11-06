import './style.css';
import { frame, congested } from './framing';
import { PhoneSurface } from './phone-surface';

const el = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const surface = new PhoneSurface();
type Phase = 'idle' | 'connecting' | 'priming' | 'streaming' | 'draining';
type Hold = {qualified: boolean; context: AudioContext | null};
type Capture = {context: AudioContext; stream: MediaStream | null; node: AudioWorkletNode | null};
type Session = {id: string; action: 'finish' | 'enter'; cancel: boolean};
let token = localStorage.getItem('ptt-device-token') || '';
const pairTicket = new URLSearchParams(location.hash.slice(1)).get('pair');
history.replaceState(null, '', location.pathname);
let phase: Phase = 'idle', hold: Hold | null = null, session: Session | null = null;
let audio: Capture | null = null, ws: WebSocket | null = null, connected = false, socketRate = 0;
let releaseEnter = false, pendingAction: string | null = null, resumeAfterDrain = false;
let generation = 0, pairing = false, ticketResolved = false;
let pairAbort: AbortController | null = null;
let heartbeat: ReturnType<typeof setInterval> | undefined, handshake: ReturnType<typeof setTimeout> | undefined;
let endTimer: ReturnType<typeof setTimeout> | undefined;
let socketWait: ((value: boolean) => void) | null = null;
const status = (value: string) => { el('status').textContent = value; };
const message = (type: string, values: Record<string, unknown> = {}) => {
  if (ws?.readyState !== WebSocket.OPEN) throw Error('电脑连接已断开');
  ws.send(JSON.stringify({type, ...values}));
};
function render() {
  surface.update(!!token && !pairing, !!hold, phase === 'priming' || phase === 'streaming', phase === 'streaming');
  el('badge').textContent = token ? '已配对' : '未配对';
  el('pair-section').hidden = !pairTicket || ticketResolved;
  el<HTMLButtonElement>('pair').disabled = pairing;
  el('re-pair').hidden = !token;
  el<HTMLButtonElement>('re-pair').disabled = pairing || !!hold || phase === 'draining';
}
function stopAudio() {
  const old = audio; audio = null;
  if (old) {
    old.node?.port.postMessage({type: 'cancel'}); old.node?.disconnect();
    old.stream?.getTracks().forEach(track => track.stop());
    void old.context.close().catch(() => {});
  }
  surface.meter(0);
}
function closeHold(pressed: Hold | null) {
  if (pressed?.context && pressed.context !== audio?.context) void pressed.context.close().catch(() => {});
}
function dropSocket() {
  clearInterval(heartbeat); clearTimeout(handshake); heartbeat = undefined; handshake = undefined;
  socketWait?.(false); socketWait = null;
  const old = ws; ws = null; connected = false; socketRate = 0; releaseEnter = false;
  old?.close();
}
function stop(reason: string) {
  generation++;
  try {
    if (session) message('cancel', {session_id: session.id});
    if (pendingAction) message('cancel', {session_id: pendingAction});
  } catch {}
  const oldHold = hold; hold = null;
  closeHold(oldHold); stopAudio(); dropSocket();
  clearTimeout(endTimer); session = null; pendingAction = null; resumeAfterDrain = false; phase = 'idle';
  surface.cancel(); status(reason); render();
}
function cancelPendingEnter() {
  if (pendingAction) { message('cancel', {session_id: pendingAction}); pendingAction = null; }
}

async function ensureSocket(rate: number, attempt: number): Promise<boolean> {
  if (connected && ws?.readyState === WebSocket.OPEN && socketRate === rate) return true;
  dropSocket(); socketRate = rate;
  const socket = new WebSocket(`wss://${location.host}/ws`); ws = socket;
  let lastReceived = performance.now();
  const ready = new Promise<boolean>(resolve => { socketWait = resolve; });
  handshake = setTimeout(() => { if (ws === socket) stop('连接超时，请检查电脑程序、网络和防火墙。'); }, 5000);
  socket.addEventListener('open', () => {
    if (generation !== attempt || ws !== socket) return;
    try { message('hello', {version: 1, token, sample_rate: rate, channels: 1}); } catch (error) { stop(String(error)); }
  });
  socket.addEventListener('message', ({data}) => {
    if (generation !== attempt || ws !== socket) return;
    let event;
    try { event = JSON.parse(data); if (!event || typeof event !== 'object') throw Error(); }
    catch { stop('电脑响应无法识别，请重新按住连接。'); return; }
    lastReceived = performance.now();
    if (event.type === 'hello' && !connected) {
      clearTimeout(handshake); connected = true;
      releaseEnter = Array.isArray(event.capabilities) && event.capabilities.includes('release_enter');
      el('mock').hidden = !event.mock; socketWait?.(true); socketWait = null;
      heartbeat = setInterval(() => {
        if (performance.now() - lastReceived >= 1000) { stop('电脑响应超时，请检查网络，恢复后重新按住。'); return; }
        try { message('heartbeat'); } catch { stop('连接已断开，请重新按住。'); }
      }, 250);
    } else if (event.type === 'ready' && event.session_id === session?.id && phase === 'priming') {
      phase = 'streaming'; status('正在向电脑输入声音。'); render();
    } else if (event.type === 'ended' && event.session_id === session?.id) {
      const resume = resumeAfterDrain; resumeAfterDrain = false;
      stopAudio(); clearTimeout(endTimer); session = null; phase = 'idle';
      pendingAction = event.complete && event.enter_pending ? event.session_id : null;
      try { if (resume) cancelPendingEnter(); } catch { stop('电脑连接已断开。'); return; }
      status(event.complete ? pendingAction ? '语音已结束，等待电脑发送回车。' : '本次语音已结束，请在电脑确认输入结果。'
        : event.reason === 'doubao_stopped' ? '豆包提前结束了录音，本次已停止，请重新按住。' : '本次输入已取消或不完整，请重新按住。');
      if (hold && resume) { if (hold.qualified) void prepare(hold); }
      else if (hold) { const old = hold; hold = null; closeHold(old); }
      render();
    } else if (event.type === 'action_result' && event.session_id === pendingAction && !session) {
      pendingAction = null;
      // A result from the previous hold must not overwrite a new hold's preparation state.
      if (!hold) status(event.status === 'executed' ? '电脑已发送回车。' : event.status === 'skipped' ? '已跳过回车，请在电脑确认输入结果。' : '回车未能完成，请在电脑确认。');
    } else if (event.type === 'error') {
      if (event.code === 'auth') { token = ''; localStorage.removeItem('ptt-device-token'); }
      stop(typeof event.message === 'string' ? event.message : '电脑拒绝了本次输入，请重新按住。');
    }
  });
  socket.addEventListener('close', () => { if (ws === socket) stop('与电脑断开，按住说话可重新连接。'); });
  socket.addEventListener('error', () => { if (ws === socket) stop('无法连接电脑，请检查证书信任、Wi-Fi 和防火墙。'); });
  return ready;
}

async function prepare(pressed: Hold) {
  if (hold !== pressed || !pressed.qualified || document.hidden) return;
  if (phase === 'draining') { resumeAfterDrain = true; status('正在结束上一轮，请保持按住。'); return; }
  if (phase === 'connecting') return;
  phase = 'connecting'; status(connected ? '正在准备麦克风。' : '正在准备麦克风和电脑连接。'); render();
  const attempt = generation;
  const current: Capture = {context: pressed.context!, stream: null, node: null}; audio = current;
  const valid = () => generation === attempt && audio === current && hold === pressed && !document.hidden;
  try {
    if (!window.isSecureContext || !navigator.mediaDevices?.getUserMedia) throw Error('麦克风需要可信 HTTPS。请先安装并信任本机证书。');
    const capture = await navigator.mediaDevices.getUserMedia({audio: {channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true}, video: false});
    if (!valid()) { capture.getTracks().forEach(track => track.stop()); return; }
    current.stream = capture;
    for (const track of capture.getTracks()) {
      track.addEventListener('ended', () => { if (audio === current) stop('麦克风被系统中断，请重新按住。'); });
      track.addEventListener('mute', () => { if (audio === current) stop('麦克风被其他应用中断，请重新按住。'); });
    }
    current.context.addEventListener('statechange', () => {
      if (audio === current && current.context.state !== 'running') stop('系统暂停了麦克风，请重新按住。');
    });
    await current.context.resume();
    if (!valid()) return;
    await current.context.audioWorklet.addModule('/audio-worklet.js');
    if (!valid()) return;
    const node = new AudioWorkletNode(current.context, 'ptt-capture', {numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [1]});
    current.node = node; current.context.createMediaStreamSource(capture).connect(node); node.connect(current.context.destination);
    node.port.onmessage = ({data}) => {
      if (audio !== current || generation !== attempt) return;
      if (data.type === 'meter') { surface.meter(Math.min(1, data.level * 5)); return; }
      const active = session;
      if (!active || data.id !== active.id) return;
      try {
        if (data.type === 'audio') {
          if (!ws || ws.readyState !== WebSocket.OPEN) throw Error('电脑连接已断开');
          if (congested(ws.bufferedAmount, current.context.sampleRate) || current.context.currentTime - data.at > .2) throw Error('网络或音频积压过多，本次已停止，请重新按住。');
          ws.send(frame(active.id, data.sequence, data.samples));
        } else if (data.type === 'end' && phase === 'draining') {
          // This callback is the barrier after every tail frame; keep the socket alive.
          if (active.cancel) message('cancel', {session_id: active.id});
          else message('end', {session_id: active.id, last_seq: data.last_seq, action: active.action});
          stopAudio();
        }
      } catch (error) { stop(error instanceof Error ? error.message : String(error)); }
    };
    if (!await ensureSocket(current.context.sampleRate, attempt) || !valid()) return;
    const id = crypto.randomUUID(); session = {id, action: 'finish', cancel: false}; pendingAction = null; phase = 'priming';
    message('start', {session_id: id}); node.port.postMessage({type: 'start', id});
    status('准备中，请开始说话。'); render();
  } catch (error) {
    if (valid()) stop(error instanceof Error ? error.message : String(error));
  }
}

surface.onDown = () => {
  if (!token || pairing || hold || document.hidden) return false;
  // Resume in the trusted pointer event. No microphone is opened until the hold qualifies.
  let context: AudioContext;
  try { context = new AudioContext({latencyHint: 'interactive'}); void context.resume().catch(() => {}); }
  catch { status('浏览器无法启动音频，请使用 Safari 或 Chrome。'); return false; }
  const pressed: Hold = {qualified: false, context}; hold = pressed;
  try { cancelPendingEnter(); } catch { stop('电脑连接已断开。'); return false; }
  if (phase === 'draining') resumeAfterDrain = true;
  setTimeout(() => { if (hold === pressed) { pressed.qualified = true; void prepare(pressed); } }, 150);
  void keepAwake(); render(); return true;
};
surface.onUp = finishOnly => {
  const pressed = hold; hold = null;
  if (phase === 'draining') { closeHold(pressed); render(); return; }
  if (!session) {
    if (phase === 'connecting') stop('连接准备已取消，请重新按住。');
    else { closeHold(pressed); status('按住说话即可开始。'); render(); }
    return;
  }
  session.cancel = phase === 'priming'; session.action = releaseEnter && !finishOnly ? 'enter' : 'finish';
  phase = 'draining'; status(session.cancel ? '正在取消本次短按。' : '正在完成输入。');
  audio?.node?.port.postMessage({type: session.cancel ? 'abort' : 'end', id: session.id});
  const id = session.id;
  endTimer = setTimeout(() => { if (session?.id === id) stop('结束确认超时，本次输入已停止。'); }, 1500);
  render();
};
surface.onCancel = () => stop('触摸已取消，本次输入已停止。');

// Screen Wake Lock is optional in browsers; it never controls microphone lifetime.
let wake: WakeLockSentinel | null = null, wakePending = false;
async function keepAwake() {
  if (document.hidden || wake || wakePending || !navigator.wakeLock) return;
  wakePending = true;
  try {
    const acquired = await navigator.wakeLock.request('screen');
    if (document.hidden) { await acquired.release(); return; }
    wake = acquired; acquired.addEventListener('release', () => { if (wake === acquired) wake = null; });
  } catch { /* Unsupported or denied by system policy; recording remains available. */ }
  finally { wakePending = false; }
}
document.addEventListener('visibilitychange', () => {
  if (document.hidden) { pairAbort?.abort(); stop('页面已离开前台，本次输入已停止。'); void wake?.release(); wake = null; }
  else void keepAwake();
});
window.addEventListener('pagehide', () => { stop('页面已关闭。'); void wake?.release(); wake = null; });
window.addEventListener('blur', () => stop('页面失去焦点，返回后请重新按住。'));
window.addEventListener('resize', () => { if (hold || session) stop('页面尺寸已变化，请重新按住。'); });
el('re-pair').addEventListener('click', () => stop('请用手机相机扫描电脑配置页的配对二维码，打开链接重新配对。'));

async function pair() {
  if (!pairTicket || pairing) return;
  pairing = true; const controller = new AbortController(); pairAbort = controller; render();
  try {
    const response = await fetch('/pair', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({ticket: pairTicket, label: '手机网页'}), signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10000)])});
    const pending = await response.json(); if (!response.ok) throw Error(pending.error);
    el('pair-status').textContent = '正在完成配对…';
    const started = Date.now();
    while (Date.now() - started < 300000) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      if (controller.signal.aborted) throw Error('配对已取消，请返回前台后重试。');
      const response = await fetch(`/pair-result/${pending.id}?poll=${encodeURIComponent(pending.poll)}`, {cache: 'no-store', signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10000)])});
      const result = await response.json(); if (!response.ok) throw Error(result.error);
      if (result.token) {
        token = result.token; ticketResolved = true; localStorage.setItem('ptt-device-token', token);
        status('配对成功，按住说话即可连接。'); void keepAwake(); return;
      }
    }
    throw Error('配对超时，请在电脑重新生成二维码。');
  } catch (error) { el('pair-status').textContent = error instanceof Error ? error.message : String(error); }
  finally { pairing = false; if (pairAbort === controller) pairAbort = null; render(); }
}
el('pair').addEventListener('click', () => { void pair(); });
void fetch('/info').then(response => response.json()).then(info => { el('computer').textContent = info.computer_name; el('mock').hidden = !info.mock; }).catch(() => status('无法读取电脑信息。'));
if (token) status('按住说话即可连接电脑。');
render(); void keepAwake();
// Opening a newly scanned link pairs just like the native scanner, including replacing an old credential.
if (pairTicket) { el('pair-section').hidden = false; void pair(); }
