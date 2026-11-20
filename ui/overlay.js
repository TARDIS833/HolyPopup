import { fitFontSize } from './helpers.mjs';
const overlay = document.getElementById('overlay');
const viewport = document.getElementById('viewport');
const content = document.getElementById('content');
let timer, animation, currentGeneration;
async function show(payload) {
  clearTimeout(timer); if (animation) animation.cancel();
  currentGeneration = payload.generation;
  document.getElementById('text').textContent = payload.text;
  document.getElementById('reference').textContent = `${payload.reference} · ${payload.version}`;
  const appearance = payload.appearance;
  overlay.className = payload.position;
  viewport.className = appearance.style;
  viewport.style.width = `${appearance.width_percent}%`; viewport.style.height = `${appearance.height_percent}%`;
  viewport.style.color = appearance.text_color;
  viewport.style.setProperty('--surface-color', appearance.color);
  viewport.style.setProperty('--surface-translucent', `${appearance.color}a6`);
  viewport.style.setProperty('--surface-glass', `${appearance.color}77`);
  viewport.style.setProperty('--surface-light', `${appearance.color}44`);
  overlay.style.fontSize = `${payload.font_size}px`; overlay.style.opacity = payload.opacity; overlay.style.visibility = 'visible';
  content.className = '';
  void content.offsetWidth;
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const padding = parseFloat(getComputedStyle(viewport).paddingTop) + parseFloat(getComputedStyle(viewport).paddingBottom);
  const availableHeight = Math.max(1, viewport.clientHeight - padding);
  if (payload.effect !== 'scroll' || reducedMotion) {
    const size = fitFontSize(payload.font_size, value => {
      overlay.style.fontSize = `${value}px`;
      return content.scrollHeight <= availableHeight && content.scrollWidth <= content.clientWidth;
    });
    overlay.style.fontSize = `${size}px`;
  }
  if (content.scrollHeight <= availableHeight) viewport.classList.add('fits');
  if (payload.effect === 'scroll' && !reducedMotion) {
    const distance = Math.max(0, content.scrollHeight - availableHeight);
    animation = distance
      ? content.animate([{transform:'translateY(0)'},{transform:`translateY(-${distance}px)`}], {duration:payload.duration_seconds * 1000, easing:'linear', fill:'forwards'})
      : content.animate([{transform:'translateY(8px)', opacity:0},{transform:'translateY(0)', opacity:1}], {duration:Math.min(1500, payload.duration_seconds * 500), easing:'ease-out', fill:'forwards'});
  } else if (['fade','slide','pop'].includes(payload.effect)) viewport.classList.add(payload.effect);
  timer = setTimeout(async () => {
    if (currentGeneration !== payload.generation) return;
    overlay.style.visibility = 'hidden';
    if (animation) { animation.cancel(); animation = undefined; }
    try { await window.__TAURI__.core.invoke('overlay_done', {generation:payload.generation}); } catch (error) { console.error(error); }
  }, payload.duration_seconds * 1000);
}
document.addEventListener('visibilitychange', () => {
  if (document.hidden) { clearTimeout(timer); if (animation) { animation.cancel(); animation = undefined; } overlay.style.visibility = 'hidden'; }
});
try {
  if (!window.__TAURI__?.event?.listen) throw new Error('앱 연결을 확인해 주세요.');
  await window.__TAURI__.event.listen('show-verse', event => show(event.payload));
  await window.__TAURI__.core.invoke('overlay_ready');
} catch (error) { console.error(error); }
