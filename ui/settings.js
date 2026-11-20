import { filteredIndexes, validSelection } from './helpers.mjs';
const $ = id => document.getElementById(id);
const demo = new URLSearchParams(location.search).has('demo');
const demoSettings = { enabled:true, interval_minutes:30, start_time:'08:00', end_time:'22:00', weekdays:[0,1,2,3,4,5,6], mode:'today', selected_index:0, version:'all', custom_verses:[], duration_seconds:12, font_size:32, opacity:0.9, position:'bottom', effect:'fade', appearance:{style:'shadow',color:'#34271f',text_color:'#ffffff',width_percent:70,height_percent:30,monitor:'primary'} };
const demoVerses = [{reference:'Psalm 23:1',text:'Yahweh is my shepherd: I shall lack nothing.',version:'WEB'}, {reference:'Psalm 46:1',text:'God is our refuge and strength, a very present help in trouble.',version:'WEB'}];
const demoMonitors = [{id:'demo-display',name:'예시 모니터'}];
let settings, verses = [], builtins = [];
const status = (message, error = false) => { $('status').textContent = message; $('status').classList.toggle('error', error); };
async function invoke(command, args) {
  if (demo) {
    if (command === 'get_state') return {settings:structuredClone(demoSettings), verses:demoVerses, monitors:demoMonitors, warning:'예시 화면입니다.'};
    if (command === 'save_settings') return {settings:structuredClone(args.settings), verses:[...demoVerses,...args.settings.custom_verses], monitors:demoMonitors, warning:'예시 화면입니다.'};
    if (command === 'refresh_monitors') return demoMonitors;
    if (command === 'preview') { status('예시 화면에서는 미리보기를 표시하지 않습니다.'); return; }
    if (command === 'quit_app') { status('예시 화면입니다.'); return; }
  }
  if (!window.__TAURI__?.core?.invoke) throw new Error('앱 연결을 확인해 주세요.');
  return window.__TAURI__.core.invoke(command, args);
}
function option(select, value, label) { const item = document.createElement('option'); item.value = value; item.textContent = label; select.append(item); }
function refreshVerses() {
  const previousVersion = $('version').value || settings.version || 'all';
  $('version').replaceChildren(); option($('version'), 'all', '모든 번역');
  for (const version of new Set(verses.map(v => v.version))) if (version !== 'all') option($('version'), version, version);
  $('version').value = [...$('version').options].some(o => o.value === previousVersion) ? previousVersion : 'all';
  const index = validSelection(verses, $('version').value, Number($('selected_index').value || settings.selected_index));
  $('selected_index').replaceChildren();
  for (const i of filteredIndexes(verses, $('version').value)) option($('selected_index'), String(i), `${verses[i].reference} · ${verses[i].version}`);
  if (index !== undefined) $('selected_index').value = String(index);
  updateVerse();
}
function updateVerse() {
  const verse = verses[Number($('selected_index').value)];
  $('sample-text').textContent = verse?.text || '선택할 말씀이 없습니다.';
  const example = $('mode').value !== 'selected';
  $('sample-ref').textContent = verse ? `${example ? '등록 구절 예시 · ' : ''}${verse.reference} · ${verse.version}` : '';
  $('verse-preview').title = example ? '실제로 표시될 말씀은 선택 방식에 따라 달라집니다.' : '직접 선택한 말씀입니다.';
  $('selected_index').disabled = $('mode').value !== 'selected';
}
function fillMonitors(monitors, selected) {
  const select = $('monitor'); select.replaceChildren();
  option(select, 'primary', '주 모니터'); option(select, 'current', '현재 화면'); option(select, 'all', '모든 모니터');
  for (const monitor of monitors) option(select, monitor.id, monitor.name);
  if (![...select.options].some(item => item.value === selected)) option(select, selected, '연결되지 않은 모니터');
  select.value = selected;
}
function fill(state) {
  settings = structuredClone(state.settings); verses = state.verses; builtins = verses.slice(0, verses.length - settings.custom_verses.length);
  for (const id of ['interval_minutes','start_time','end_time','mode','duration_seconds','font_size','opacity','position','effect']) $(id).value = settings[id];
  $('enabled').checked = settings.enabled;
  for (const id of ['style','color','text_color','width_percent','height_percent']) $(id).value = settings.appearance[id];
  fillMonitors(state.monitors, settings.appearance.monitor);
  document.querySelectorAll('.weekdays input').forEach(input => { input.checked = settings.weekdays.includes(Number(input.value)); });
  $('version').replaceChildren(); option($('version'), settings.version || 'all', ''); $('version').value = settings.version || 'all';
  $('selected_index').replaceChildren(); option($('selected_index'), String(settings.selected_index), ''); $('selected_index').value = String(settings.selected_index);
  refreshVerses(); updateRanges();
  status(state.warning || '');
}
function collect() {
  const result = {...settings, enabled:$('enabled').checked, weekdays:[...document.querySelectorAll('.weekdays input:checked')].map(x => Number(x.value)), version:$('version').value, selected_index:Number($('selected_index').value)};
  for (const id of ['start_time','end_time','mode','position','effect']) result[id] = $(id).value;
  for (const id of ['interval_minutes','duration_seconds','font_size','opacity']) result[id] = Number($(id).value);
  result.appearance = {style:$('style').value, color:$('color').value, text_color:$('text_color').value, width_percent:Number($('width_percent').value), height_percent:Number($('height_percent').value), monitor:$('monitor').value};
  if (!result.weekdays.length && result.enabled) throw new Error('자동 표시할 요일을 하나 이상 선택해 주세요.');
  for (const id of ['interval_minutes','duration_seconds','font_size','opacity','start_time','end_time']) if (!$(id).checkValidity()) throw new Error('시간과 표시 설정을 확인해 주세요.');
  if (!$('selected_index').options.length) throw new Error('선택할 말씀이 없습니다.');
  return result;
}
function updateRanges() { $('font-value').textContent = `${$('font_size').value}px`; $('opacity-value').textContent = `${Math.round(Number($('opacity').value)*100)}%`; $('width-value').textContent = `${$('width_percent').value}%`; $('height-value').textContent = `${$('height_percent').value}%`; }
document.querySelectorAll('.tabs button').forEach(button => button.addEventListener('click', () => {
  document.querySelectorAll('.tabs button').forEach(b => { b.classList.toggle('active', b === button); if (b === button) b.setAttribute('aria-current','page'); else b.removeAttribute('aria-current'); });
  document.querySelectorAll('.panel').forEach(panel => { panel.hidden = panel.id !== button.dataset.tab; });
}));
$('mode').addEventListener('change', updateVerse); $('version').addEventListener('change', refreshVerses); $('selected_index').addEventListener('change', updateVerse);
for (const id of ['font_size','opacity','width_percent','height_percent']) $(id).addEventListener('input', updateRanges);
$('refresh-monitors').addEventListener('click', async () => { try { fillMonitors(await invoke('refresh_monitors'), $('monitor').value); status('모니터 목록을 새로고침했습니다.'); } catch (error) { status(String(error), true); } });
$('add-custom').addEventListener('click', () => {
  const verse = {version:$('custom-version').value.trim(), reference:$('custom-reference').value.trim(), text:$('custom-text').value.trim()};
  if (!verse.version || !verse.reference || !verse.text) return status('번역, 구절, 말씀을 모두 입력해 주세요.', true);
  if (verse.version === 'all' || new TextEncoder().encode(verse.text).length > 10000 || new TextEncoder().encode(verse.reference).length > 200 || new TextEncoder().encode(verse.version).length > 100) return status('입력한 말씀의 길이 또는 번역 이름을 확인해 주세요.', true);
  settings.custom_verses.push(verse); verses = [...builtins, ...settings.custom_verses];
  $('custom-version').value = ''; $('custom-reference').value = ''; $('custom-text').value = '';
  $('mode').value = 'selected';
  option($('version'), verse.version, verse.version); $('version').value = verse.version;
  option($('selected_index'), String(verses.length - 1), verse.reference); $('selected_index').value = String(verses.length - 1);
  refreshVerses(); $('add-custom').closest('details').open = false;
  status('말씀을 추가했습니다. 저장하면 적용됩니다.');
});
$('save').addEventListener('click', async () => { try { const state = await invoke('save_settings', {settings:collect()}); fill(state); status(state.warning || '설정을 저장했습니다.'); } catch (error) { status(String(error), true); } });
$('preview').addEventListener('click', async () => { try { await invoke('preview', {settings:collect()}); if (!demo) status('미리보기를 표시했습니다.'); } catch (error) { status(String(error), true); } });
$('quit').addEventListener('click', async () => { try { await invoke('quit_app'); } catch (error) { status(String(error), true); } });
try { fill(await invoke('get_state')); } catch (error) { status(String(error), true); document.querySelectorAll('button:not([data-tab])').forEach(b => b.disabled = true); }
