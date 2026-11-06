/** Decorative waves never participate in hit testing: the operation radius is 90 CSS px. */
export class PhoneSurface {
  readonly element = document.getElementById('talk')!;
  onDown: (() => boolean) | undefined;
  onUp: ((finishOnly: boolean) => void) | undefined;
  onCancel: (() => void) | undefined;
  private canvas = document.getElementById('ripples') as HTMLCanvasElement;
  private circle = document.getElementById('operation-circle')!;
  private hint = document.getElementById('idle-hint')!;
  private actions = document.getElementById('actions')!;
  private pointer: number | null = null;
  private active = false;
  private paired = false;
  private anchor = {x: 0, y: 0};
  private started = 0;
  private last = 0;
  private level = 0;
  private smoothed = 0;
  private voice = false;
  private voiceStarted = 0;
  private raf = 0;
  private reduce = matchMedia('(prefers-reduced-motion: reduce)');

  constructor() {
    this.element.addEventListener('pointerdown', e => {
      if (!this.paired || this.pointer !== null || !e.isPrimary || e.button !== 0 || (e.target as Element).closest('button,input,a')) return;
      e.preventDefault();
      this.pointer = e.pointerId;
      try { this.element.setPointerCapture(e.pointerId); } catch { this.pointer = null; return; }
      const bounds = this.element.getBoundingClientRect();
      this.anchor = {x: e.clientX - bounds.left, y: e.clientY - bounds.top};
      this.active = true; this.started = performance.now(); this.last = this.started; this.smoothed = 0;
      this.circle.style.left = `${this.anchor.x}px`; this.circle.style.top = `${this.anchor.y}px`;
      // Start a fresh transition even if the previous hold is still fading out.
      this.circle.getAnimations().forEach(animation => animation.cancel());
      this.circle.style.opacity = '1';
      this.canvas.getAnimations().forEach(animation => animation.cancel()); this.canvas.style.opacity = '1';
      if (!this.reduce.matches) this.circle.animate([{opacity: 0}, {opacity: 1}], {duration: 300, easing: 'ease-in-out'});
      this.hint.classList.add('is-hidden');
      if (this.onDown?.() !== true) this.cancel();
      else this.tick();
    });
    this.element.addEventListener('pointermove', e => { if (e.pointerId === this.pointer) e.preventDefault(); });
    this.element.addEventListener('pointerup', e => {
      if (e.pointerId !== this.pointer) return;
      e.preventDefault(); this.pointer = null;
      if (!this.active) return;
      const bounds = this.element.getBoundingClientRect(), x = e.clientX - bounds.left, y = e.clientY - bounds.top;
      const outside = x < 0 || y < 0 || x >= bounds.width || y >= bounds.height || Math.hypot(x - this.anchor.x, y - this.anchor.y) > 90;
      this.finish(true); this.onUp?.(outside);
    });
    const cancel = (e: PointerEvent) => {
      if (e.pointerId !== this.pointer) return;
      this.pointer = null;
      if (this.active) { this.cancel(); this.onCancel?.(); }
    };
    this.element.addEventListener('pointercancel', cancel);
    this.element.addEventListener('lostpointercapture', cancel);
    this.element.addEventListener('contextmenu', e => e.preventDefault());
  }
  update(paired: boolean, holding: boolean, voice: boolean, streaming: boolean) {
    this.paired = paired;
    if (voice && !this.voice) this.voiceStarted = performance.now();
    this.voice = voice;
    this.element.setAttribute('aria-disabled', String(!paired));
    this.element.setAttribute('aria-pressed', String(holding));
    this.hint.classList.toggle('is-hidden', !paired || holding);
    this.actions.classList.toggle('is-visible', streaming && holding);
    this.actions.setAttribute('aria-hidden', String(!streaming || !holding));
    if (this.active && !holding) this.cancel();
  }
  meter(value: number) { this.level = Number.isFinite(value) ? Math.sqrt(Math.min(1, Math.max(0, value - .008) * 4)) : 0; }
  cancel() { this.finish(false); }
  private finish(animated: boolean) {
    this.active = false; this.level = 0;
    this.actions.classList.remove('is-visible'); this.actions.setAttribute('aria-hidden', 'true');
    this.hint.classList.toggle('is-hidden', !this.paired);
    const opacity = getComputedStyle(this.circle).opacity;
    this.circle.getAnimations().forEach(animation => animation.cancel()); this.circle.style.opacity = '0';
    if (animated && !this.reduce.matches) this.circle.animate([{opacity}, {opacity: 0}], {duration: 300, easing: 'ease-in-out'});
    cancelAnimationFrame(this.raf); this.raf = 0;
    this.canvas.getAnimations().forEach(animation => animation.cancel());
    if (animated && !this.reduce.matches) {
      this.canvas.style.opacity = '0'; this.canvas.animate([{opacity: 1}, {opacity: 0}], {duration: 300, easing: 'ease-in-out'});
    } else {
      this.canvas.style.opacity = '0';
      const context = this.canvas.getContext('2d'); context?.clearRect(0, 0, this.canvas.width, this.canvas.height);
    }
  }
  private tick = () => {
    if (!this.active) return;
    const now = performance.now(), dt = Math.min(.1, (now - this.last) / 1000); this.last = now;
    const goal = this.voice ? this.level : 0;
    this.smoothed += (goal - this.smoothed) * (1 - Math.exp(-dt / (goal > this.smoothed ? .06 : .22)));
    const bounds = this.element.getBoundingClientRect(), scale = devicePixelRatio || 1;
    const width = Math.round(bounds.width * scale), height = Math.round(bounds.height * scale);
    if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
    const context = this.canvas.getContext('2d');
    if (context) {
      context.setTransform(scale, 0, 0, scale, 0, 0); context.clearRect(0, 0, bounds.width, bounds.height);
      if (this.voice && !this.reduce.matches) {
        const t = (now - this.voiceStarted) / 1000, fade = Math.min(1, (now - this.started) / 300);
        const ring = (radius: number, alpha: number) => {
          context.strokeStyle = `rgba(35,75,198,${alpha * fade})`; context.lineWidth = 4;
          context.beginPath(); context.arc(this.anchor.x, this.anchor.y, radius, 0, Math.PI * 2); context.stroke();
        };
        for (let i = 0; i < 3; i++) {
          if (t < i * .4) continue;
          const p = ((t - i * .4) / 1.2 % 1 + 1) % 1;
          ring(90 + p * 90, this.smoothed * .6 * (1 - p));
        }
        const quiet = t % 2 / 1.6;
        if (quiet <= 1) ring(90 + quiet * 22, .14 * Math.max(0, 1 - this.smoothed * 3) * (1 - quiet));
      }
    }
    // No animation work while idle. Reduced motion still displays the fixed circle.
    this.raf = requestAnimationFrame(this.tick);
  };
}
