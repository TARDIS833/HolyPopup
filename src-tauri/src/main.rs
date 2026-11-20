#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Datelike, Local, Timelike};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, WebviewWindow};

#[derive(Clone, Serialize, Deserialize)]
struct Verse {
    reference: String,
    text: String,
    version: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Appearance {
    style: String,
    color: String,
    text_color: String,
    width_percent: u32,
    height_percent: u32,
    monitor: String,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            style: "shadow".into(),
            color: "#34271f".into(),
            text_color: "#ffffff".into(),
            width_percent: 70,
            height_percent: 30,
            monitor: "primary".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    enabled: bool,
    interval_minutes: u32,
    start_time: String,
    end_time: String,
    weekdays: Vec<u32>,
    mode: String,
    version: String,
    selected_index: usize,
    custom_verses: Vec<Verse>,
    duration_seconds: u32,
    font_size: u32,
    opacity: f64,
    position: String,
    effect: String,
    #[serde(default)]
    appearance: Appearance,
}
#[derive(Serialize)]
struct AppState {
    settings: Settings,
    verses: Vec<Verse>,
    warning: Option<String>,
    monitors: Vec<MonitorInfo>,
}
#[derive(Serialize)]
struct MonitorInfo {
    id: String,
    name: String,
}
enum Wake {
    Reset,
    Hide(Instant, u64),
}
struct Shared {
    settings: Mutex<Settings>,
    warning: Mutex<Option<String>>,
    tx: mpsc::Sender<Wake>,
    generation: AtomicU64,
    display: Mutex<DisplayState>,
    show_lock: Mutex<()>,
    pause_until: Mutex<Option<SystemTime>>,
    pause_action: AtomicU64,
}
#[derive(Default)]
struct DisplayState {
    ready: HashSet<String>,
    pending: HashMap<String, DisplayVerse>,
    deadline: Option<(Instant, u64)>,
}
#[derive(Clone, Serialize)]
struct DisplayVerse {
    generation: u64,
    text: String,
    reference: String,
    version: String,
    effect: String,
    duration_seconds: u32,
    font_size: u32,
    opacity: f64,
    position: String,
    appearance: Appearance,
}

fn samples() -> Vec<Verse> {
    [
        ("Psalm 23:1", "Yahweh is my shepherd: I shall lack nothing."),
        ("Psalm 46:1", "God is our refuge and strength, a very present help in trouble."),
        ("Psalm 119:105", "Your word is a lamp to my feet, and a light for my path."),
        ("Proverbs 3:5", "Trust in Yahweh with all your heart, and don't lean on your own understanding."),
        ("Isaiah 41:10", "Don't you be afraid, for I am with you. Don't be dismayed, for I am your God. I will strengthen you. Yes, I will help you. Yes, I will uphold you with the right hand of my righteousness."),
        ("Matthew 5:14", "You are the light of the world. A city located on a hill can't be hidden."),
        ("John 14:27", "Peace I leave with you. My peace I give to you; not as the world gives, I give to you. Don’t let your heart be troubled, neither let it be fearful."),
        ("Romans 12:12", "rejoicing in hope, enduring in troubles, continuing steadfastly in prayer"),
        ("Philippians 4:13", "I can do all things through Christ who strengthens me."),
        ("1 Thessalonians 5:16", "Always rejoice.")
    ].into_iter().map(|(reference, text)| Verse { reference: reference.into(), text: text.into(), version: "WEB".into() }).collect()
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 30,
            start_time: "08:00".into(),
            end_time: "22:00".into(),
            weekdays: (0..7).collect(),
            mode: "today".into(),
            version: "all".into(),
            selected_index: 0,
            custom_verses: vec![],
            duration_seconds: 12,
            font_size: 32,
            opacity: 0.9,
            position: "bottom".into(),
            effect: "fade".into(),
            appearance: Appearance::default(),
        }
    }
}
fn verses(s: &Settings) -> Vec<Verse> {
    let mut v = samples();
    v.extend(s.custom_verses.clone());
    v
}
fn minutes(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2
        || m.len() != 2
        || !h.bytes().all(|c| c.is_ascii_digit())
        || !m.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}
fn valid_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
fn validate(s: &Settings) -> Result<(), String> {
    if !(1..=1440).contains(&s.interval_minutes)
        || minutes(&s.start_time).is_none()
        || minutes(&s.end_time).is_none()
        || s.weekdays.iter().any(|d| *d > 6)
        || !["today", "selected", "random"].contains(&s.mode.as_str())
        || s.selected_index >= 10 + s.custom_verses.len()
        || !(1..=600).contains(&s.duration_seconds)
        || !(12..=120).contains(&s.font_size)
        || !s.opacity.is_finite()
        || !(0.1..=1.0).contains(&s.opacity)
        || !["top", "center", "bottom"].contains(&s.position.as_str())
        || !["fade", "none", "slide", "pop", "scroll"].contains(&s.effect.as_str())
        || (s.version != "all" && !verses(s).iter().any(|v| v.version == s.version))
        || !["shadow", "plain", "glass", "translucent", "color"]
            .contains(&s.appearance.style.as_str())
        || !valid_color(&s.appearance.color)
        || !valid_color(&s.appearance.text_color)
        || !(10..=100).contains(&s.appearance.width_percent)
        || !(10..=100).contains(&s.appearance.height_percent)
        || s.appearance.monitor.is_empty()
        || s.appearance.monitor.len() > 512
        || s.custom_verses.len() > 1000
        || s.custom_verses.iter().any(|v| {
            v.text.trim().is_empty()
                || v.reference.trim().is_empty()
                || v.version.trim().is_empty()
                || v.text.len() > 10000
                || v.reference.len() > 200
                || v.version.len() > 100
        })
    {
        return Err("설정값을 확인해 주세요.".into());
    }
    if s.enabled && s.weekdays.is_empty() {
        return Err("자동 표시할 요일을 하나 이상 선택해 주세요.".into());
    }
    if s.mode == "selected"
        && s.version != "all"
        && verses(s)[s.selected_index].version != s.version
    {
        return Err("선택한 번역에 속하는 구절을 선택해 주세요.".into());
    }
    Ok(())
}
fn active(s: &Settings, day: u32, now: u32) -> bool {
    let start = minutes(&s.start_time).unwrap();
    let end = minutes(&s.end_time).unwrap();
    if start == end {
        return s.weekdays.contains(&day);
    }
    if start < end {
        s.weekdays.contains(&day) && now >= start && now < end
    } else if now >= start {
        s.weekdays.contains(&day)
    } else {
        now < end && s.weekdays.contains(&((day + 6) % 7))
    }
}
fn monitor_list(window: &WebviewWindow) -> Result<Vec<(String, tauri::window::Monitor)>, String> {
    let mut monitors = window.available_monitors().map_err(|e| e.to_string())?;
    monitors.sort_by_key(|m| {
        (
            m.name().cloned().unwrap_or_default(),
            m.position().x,
            m.position().y,
        )
    });
    let mut counts: HashMap<String, usize> = HashMap::new();
    Ok(monitors
        .into_iter()
        .map(|m| {
            let name = m.name().cloned().unwrap_or_else(|| "모니터".into());
            let count = counts.entry(name.clone()).or_default();
            let id = format!("monitor:{name}:{}", *count);
            *count += 1;
            (id, m)
        })
        .collect())
}
fn monitor_info(window: &WebviewWindow) -> Result<Vec<MonitorInfo>, String> {
    Ok(monitor_list(window)?
        .into_iter()
        .enumerate()
        .map(|(index, (id, m))| MonitorInfo {
            name: format!(
                "{} · {} · {}×{}",
                index + 1,
                m.name().map(String::as_str).unwrap_or("모니터"),
                m.size().width,
                m.size().height
            ),
            id,
        })
        .collect())
}
fn state(window: &WebviewWindow, shared: &Shared) -> AppState {
    let settings = shared.settings.lock().unwrap().clone();
    AppState {
        verses: verses(&settings),
        settings,
        warning: shared.warning.lock().unwrap().clone(),
        monitors: monitor_info(window).unwrap_or_default(),
    }
}
#[tauri::command]
fn refresh_monitors(window: WebviewWindow) -> Result<Vec<MonitorInfo>, String> {
    main_only(&window)?;
    monitor_info(&window)
}

fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("허용되지 않은 창입니다.".into())
    }
}
fn overlay_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "overlay" || window.label().starts_with("overlay-") {
        Ok(())
    } else {
        Err("허용되지 않은 창입니다.".into())
    }
}
#[tauri::command]
fn get_state(window: WebviewWindow, shared: tauri::State<'_, Shared>) -> Result<AppState, String> {
    main_only(&window)?;
    Ok(state(&window, &shared))
}
#[tauri::command]
fn save_settings(
    window: WebviewWindow,
    app: tauri::AppHandle,
    shared: tauri::State<'_, Shared>,
    settings: Settings,
) -> Result<AppState, String> {
    main_only(&window)?;
    validate(&settings)?;
    let mut saved = shared.settings.lock().unwrap();
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    persist_settings(&dir, &settings)?;
    *saved = settings;
    drop(saved);
    *shared.warning.lock().unwrap() = None;
    let _ = shared.tx.send(Wake::Reset);
    Ok(state(&window, &shared))
}
fn persist_settings(dir: &Path, settings: &Settings) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let data = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    if data.len() > 10_000_000 {
        return Err("설정 데이터가 너무 큽니다.".into());
    }
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = dir.join(format!("settings-{id:039}.json"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(&data)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn choose_index(s: &Settings, list: &[Verse], day: usize, random: usize) -> usize {
    if s.mode == "selected" {
        return s.selected_index;
    }
    let eligible: Vec<_> = list
        .iter()
        .enumerate()
        .filter(|(_, v)| s.version == "all" || v.version == s.version)
        .map(|(i, _)| i)
        .collect();
    eligible[if s.mode == "random" {
        random % eligible.len()
    } else {
        day % eligible.len()
    }]
}
fn overlay_windows(app: &tauri::AppHandle) -> Vec<WebviewWindow> {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label == "overlay" || label.starts_with("overlay-"))
        .map(|(_, w)| w)
        .collect()
}
fn select_monitors(
    ids: &[String],
    choice: &str,
    primary: Option<usize>,
    current: Option<usize>,
) -> Vec<usize> {
    if ids.is_empty() {
        return vec![];
    }
    let fallback = primary.filter(|i| *i < ids.len()).unwrap_or(0);
    if choice == "all" {
        return (0..ids.len()).collect();
    }
    let target = match choice {
        "primary" => Some(fallback),
        "current" => current.filter(|i| *i < ids.len()),
        _ => ids.iter().position(|id| id == choice),
    };
    vec![target.unwrap_or(fallback)]
}
fn show(app: &tauri::AppHandle, s: &Settings, automatic: bool) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let _show = shared.show_lock.lock().unwrap();
    if automatic && paused(*shared.pause_until.lock().unwrap(), SystemTime::now()) {
        return Ok(());
    }
    let main = app
        .get_webview_window("main")
        .ok_or("설정 창을 찾을 수 없습니다.")?;
    let monitors = monitor_list(&main)?;
    if monitors.is_empty() {
        return Err("사용 가능한 모니터가 없습니다.".into());
    }
    let primary = main.primary_monitor().map_err(|e| e.to_string())?;
    let current = main.current_monitor().map_err(|e| e.to_string())?;
    let fallback = primary
        .as_ref()
        .and_then(|p| {
            monitors
                .iter()
                .position(|(_, m)| m.position() == p.position() && m.size() == p.size())
        })
        .unwrap_or(0);
    let current_index = current.as_ref().and_then(|p| {
        monitors
            .iter()
            .position(|(_, m)| m.position() == p.position() && m.size() == p.size())
    });
    let ids: Vec<_> = monitors.iter().map(|(id, _)| id.clone()).collect();
    let selected = select_monitors(&ids, &s.appearance.monitor, Some(fallback), current_index);
    // Build before taking the display lock: window setup may invoke overlay_ready.
    let mut windows = Vec::new();
    for i in 0..selected.len() {
        let label = if i == 0 {
            "overlay".to_owned()
        } else {
            format!("overlay-{i}")
        };
        let w = if let Some(w) = app.get_webview_window(&label) {
            w
        } else {
            tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::App("overlay.html".into()),
            )
            .title("HolyPop Overlay")
            .visible(false)
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .resizable(false)
            .shadow(false)
            .build()
            .map_err(|e| e.to_string())?
        };
        w.set_ignore_cursor_events(true)
            .map_err(|e| e.to_string())?;
        windows.push(w);
    }
    let mut display = shared.display.lock().unwrap();
    display.pending.clear();
    for w in overlay_windows(app) {
        let _ = w.hide();
    }
    let list = verses(s);
    let index = choose_index(
        s,
        &list,
        Local::now().date_naive().num_days_from_ce().unsigned_abs() as usize,
        rand::thread_rng().gen_range(
            0..list
                .iter()
                .filter(|v| s.version == "all" || v.version == s.version)
                .count(),
        ),
    );
    let v = &list[index];
    let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let payload = DisplayVerse {
        generation,
        text: v.text.clone(),
        reference: v.reference.clone(),
        version: v.version.clone(),
        effect: s.effect.clone(),
        duration_seconds: s.duration_seconds,
        font_size: s.font_size,
        opacity: s.opacity,
        position: s.position.clone(),
        appearance: s.appearance.clone(),
    };
    let result: Result<(), String> = (|| {
        for (i, w) in windows.iter().enumerate() {
            let monitor = &monitors[selected[i]].1;
            w.set_position(*monitor.position())
                .map_err(|e| e.to_string())?;
            w.set_size(*monitor.size()).map_err(|e| e.to_string())?;
            if display.ready.contains(w.label()) {
                w.show().map_err(|e| e.to_string())?;
                w.emit("show-verse", payload.clone())
                    .map_err(|e| e.to_string())?;
            } else {
                display
                    .pending
                    .insert(w.label().to_owned(), payload.clone());
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        display.pending.clear();
        for w in overlay_windows(app) {
            let _ = w.hide();
        }
        return Err(error);
    }
    let deadline = Instant::now() + Duration::from_secs(u64::from(s.duration_seconds) + 2);
    display.deadline = Some((deadline, generation));
    let _ = shared.tx.send(Wake::Hide(deadline, generation));
    Ok(())
}
#[tauri::command]
async fn preview(
    window: WebviewWindow,
    app: tauri::AppHandle,
    settings: Settings,
) -> Result<(), String> {
    main_only(&window)?;
    validate(&settings)?;
    tauri::async_runtime::spawn_blocking(move || show(&app, &settings, false))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn overlay_ready(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    overlay_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<Shared>();
        let mut display = shared.display.lock().unwrap();
        display.ready.insert(window.label().to_owned());
        if let Some(payload) = display.pending.remove(window.label()) {
            if payload.generation == shared.generation.load(Ordering::SeqCst) {
                window.show().map_err(|e| e.to_string())?;
                let deadline =
                    Instant::now() + Duration::from_secs(u64::from(payload.duration_seconds) + 2);
                let generation = payload.generation;
                if let Err(e) = window.emit("show-verse", payload) {
                    let _ = window.hide();
                    return Err(e.to_string());
                }
                display.deadline = Some((deadline, generation));
                let _ = shared.tx.send(Wake::Hide(deadline, generation));
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn overlay_done(
    window: WebviewWindow,
    app: tauri::AppHandle,
    generation: u64,
) -> Result<(), String> {
    overlay_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<Shared>();
        let _display = shared.display.lock().unwrap();
        if shared.generation.load(Ordering::SeqCst) == generation {
            window.hide().map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn quit_app(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    main_only(&window)?;
    app.exit(0);
    Ok(())
}

fn load(app: &tauri::AppHandle) -> (Settings, Option<String>) {
    let Ok(dir) = app.path().app_data_dir() else {
        return (
            Settings::default(),
            Some("설정 폴더를 찾을 수 없습니다.".into()),
        );
    };
    load_from_dir(&dir)
}
fn load_from_dir(dir: &Path) -> (Settings, Option<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            return (
                Settings::default(),
                (error.kind() != std::io::ErrorKind::NotFound)
                    .then(|| format!("설정 폴더를 읽을 수 없습니다: {error}")),
            )
        }
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("settings-") && n.ends_with(".json"))
        })
        .collect();
    paths.sort();
    paths.reverse();
    let mut invalid = false;
    for path in paths {
        if fs::metadata(&path).is_ok_and(|m| m.len() <= 10_000_000) {
            if let Ok(data) = fs::read(path) {
                if let Ok(s) = serde_json::from_slice::<Settings>(&data) {
                    if validate(&s).is_ok() {
                        return (
                            s,
                            invalid.then(|| {
                                "최근 설정을 읽을 수 없어 이전 설정을 복구했습니다.".into()
                            }),
                        );
                    }
                }
            }
        }
        invalid = true;
    }
    (
        Settings::default(),
        invalid.then(|| {
            "저장된 설정을 읽을 수 없어 기본 설정을 사용합니다. 기존 파일은 보존되어 있습니다."
                .into()
        }),
    )
}
fn due(s: &Settings, day: u32, minute: u32, elapsed: Duration) -> bool {
    s.enabled
        && elapsed >= Duration::from_secs(u64::from(s.interval_minutes) * 60)
        && active(s, day, minute)
}
fn automatic_due(
    s: &Settings,
    day: u32,
    minute: u32,
    elapsed: Duration,
    deadline: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    !paused(deadline, now) && due(s, day, minute, elapsed)
}
fn paused(deadline: Option<SystemTime>, now: SystemTime) -> bool {
    deadline.is_some_and(|deadline| now < deadline)
}
fn pause_deadline(now: SystemTime, minutes: u64) -> SystemTime {
    now + Duration::from_secs(minutes * 60)
}
struct PauseMenu {
    status: tauri::menu::MenuItem<tauri::Wry>,
    resume: tauri::menu::MenuItem<tauri::Wry>,
}
fn update_pause_menu(app: &tauri::AppHandle, deadline: Option<SystemTime>) {
    let menu = app.state::<PauseMenu>();
    let text = match deadline {
        Some(end) => format!(
            "자동 표시 중지 · {}까지",
            chrono::DateTime::<Local>::from(end).format("%m/%d %H:%M")
        ),
        None => "일시 중지 안 함".into(),
    };
    if let Err(e) = menu
        .status
        .set_text(text)
        .and_then(|_| menu.resume.set_enabled(deadline.is_some()))
    {
        eprintln!("트레이 상태 갱신 실패: {e}");
    }
}
fn set_pause(app: &tauri::AppHandle, minutes: Option<u64>, action: u64) {
    let shared = app.state::<Shared>();
    let _show = shared.show_lock.lock().unwrap();
    if shared.pause_action.load(Ordering::SeqCst) != action {
        return;
    }
    let deadline = minutes.map(|minutes| pause_deadline(SystemTime::now(), minutes));
    *shared.pause_until.lock().unwrap() = deadline;
    if deadline.is_some() {
        let mut display = shared.display.lock().unwrap();
        shared.generation.fetch_add(1, Ordering::SeqCst);
        display.pending.clear();
        display.deadline = None;
        for window in overlay_windows(app) {
            if let Err(e) = window.hide() {
                eprintln!("오버레이 숨기기 실패: {e}");
            }
        }
    }
    update_pause_menu(app, deadline);
    let _ = shared.tx.send(Wake::Reset);
}
fn hide_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.hide() {
            eprintln!("설정 창 숨기기 실패: {e}");
            return;
        }
        #[cfg(target_os = "macos")]
        if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Accessory) {
            eprintln!("Dock 숨기기 실패: {e}");
        }
    }
}
fn restore_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
            eprintln!("Dock 복원 실패: {e}");
        }
        if let Err(e) = window
            .unminimize()
            .and_then(|_| window.show())
            .and_then(|_| window.set_focus())
        {
            eprintln!("설정 창 열기 실패: {e}");
        }
    }
}
fn tray_icon_image() -> tauri::image::Image<'static> {
    let mut pixels = vec![0u8; 20 * 20 * 4];
    for y in 0..20 {
        for x in 0..20 {
            if ((8..12).contains(&x) && (3..17).contains(&y))
                || ((4..16).contains(&x) && (7..11).contains(&y))
            {
                let index = (y * 20 + x) * 4;
                pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    tauri::image::Image::new_owned(pixels, 20, 20)
}
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::TrayIconBuilder,
    };
    let pause30 = MenuItem::with_id(app, "pause_30", "30분 동안 꺼짐", true, None::<&str>)?;
    let pause60 = MenuItem::with_id(app, "pause_60", "1시간 동안 꺼짐", true, None::<&str>)?;
    let pause120 = MenuItem::with_id(app, "pause_120", "2시간 동안 꺼짐", true, None::<&str>)?;
    let pause240 = MenuItem::with_id(app, "pause_240", "4시간 동안 꺼짐", true, None::<&str>)?;
    let pause480 = MenuItem::with_id(app, "pause_480", "8시간 동안 꺼짐", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let status = MenuItem::with_id(app, "pause_status", "일시 중지 안 함", false, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "지금 재개", false, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let open = MenuItem::with_id(app, "open_settings", "설정 화면 열기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "exit_app", "종료하기", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &pause30,
            &pause60,
            &pause120,
            &pause240,
            &pause480,
            &separator,
            &status,
            &resume,
            &separator2,
            &open,
            &quit,
        ],
    )?;
    app.manage(PauseMenu { status, resume });
    #[cfg(target_os = "macos")]
    let icon = tray_icon_image();
    #[cfg(not(target_os = "macos"))]
    let icon = app
        .default_window_icon()
        .map(|icon| icon.clone().to_owned())
        .unwrap_or_else(tray_icon_image);
    TrayIconBuilder::with_id("holypop")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("HolyPop")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_settings" => restore_main(app),
            "exit_app" => app.exit(0),
            id => {
                let minutes = match id {
                    "pause_30" => Some(Some(30)),
                    "pause_60" => Some(Some(60)),
                    "pause_120" => Some(Some(120)),
                    "pause_240" => Some(Some(240)),
                    "pause_480" => Some(Some(480)),
                    "resume" => Some(None),
                    _ => None,
                };
                if let Some(minutes) = minutes {
                    let action = app
                        .state::<Shared>()
                        .pause_action
                        .fetch_add(1, Ordering::SeqCst)
                        + 1;
                    let app = app.clone();
                    tauri::async_runtime::spawn_blocking(move || set_pause(&app, minutes, action));
                }
            }
        })
        .build(app)?;
    Ok(())
}
#[cfg(target_os = "macos")]
fn observe_minimize(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::NSWindowDidMiniaturizeNotification;
    use objc2_foundation::{NSNotification, NSNotificationCenter};
    use std::ptr::NonNull;
    let window = app
        .get_webview_window("main")
        .ok_or("설정 창을 찾을 수 없습니다.")?;
    let native = window.ns_window()? as *mut AnyObject;
    if native.is_null() {
        return Err("macOS 창을 찾을 수 없습니다.".into());
    }
    let handle = app.clone();
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        hide_main(&handle);
    });
    // NotificationCenter retains the observer for the lifetime of this application.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSWindowDidMiniaturizeNotification),
            Some(&*native),
            None,
            &block,
        );
    }
    Ok(())
}
fn main() {
    let (tx, rx) = mpsc::channel();
    tauri::Builder::default()
        .setup(move |app| {
            let (settings, warning) = load(app.handle());
            app.manage(Shared {
                settings: Mutex::new(settings),
                warning: Mutex::new(warning),
                tx,
                generation: AtomicU64::new(0),
                display: Mutex::new(DisplayState::default()),
                show_lock: Mutex::new(()),
                pause_until: Mutex::new(None),
                pause_action: AtomicU64::new(0),
            });
            setup_tray(app)?;
            #[cfg(target_os = "macos")]
            observe_minimize(app.handle())?;
            if let Some(overlay) = app.get_webview_window("overlay") {
                overlay.set_ignore_cursor_events(true)?;
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last = Instant::now();
                let mut hide: Option<(Instant, u64)> = None;
                loop {
                    let timeout = hide
                        .map(|(at, _)| {
                            at.saturating_duration_since(Instant::now())
                                .min(Duration::from_secs(30))
                        })
                        .unwrap_or(Duration::from_secs(30));
                    let pause_until = *handle.state::<Shared>().pause_until.lock().unwrap();
                    let timeout = pause_until
                        .map(|end| {
                            end.duration_since(SystemTime::now())
                                .unwrap_or_default()
                                .min(timeout)
                        })
                        .unwrap_or(timeout);
                    match rx.recv_timeout(timeout) {
                        Ok(Wake::Reset) => {
                            last = Instant::now();
                        }
                        Ok(Wake::Hide(at, generation)) => {
                            hide = Some((at, generation));
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Some((at, generation)) = hide {
                        if Instant::now() >= at {
                            let shared = handle.state::<Shared>();
                            let mut display = shared.display.lock().unwrap();
                            if shared.generation.load(Ordering::SeqCst) == generation {
                                if let Some((deadline, g)) = display.deadline {
                                    if g == generation && deadline > Instant::now() {
                                        hide = Some((deadline, g));
                                        continue;
                                    }
                                }
                                display.pending.clear();
                                display.deadline = None;
                                for w in overlay_windows(&handle) {
                                    let _ = w.hide();
                                }
                            }
                            hide = None;
                        }
                    }
                    let shared = handle.state::<Shared>();
                    {
                        let _show = shared.show_lock.lock().unwrap();
                        let mut deadline = shared.pause_until.lock().unwrap();
                        if deadline.is_some() && !paused(*deadline, SystemTime::now()) {
                            *deadline = None;
                            update_pause_menu(&handle, None);
                            last = Instant::now();
                        }
                    }
                    let s = shared.settings.lock().unwrap().clone();
                    let now = Local::now();
                    let deadline = *shared.pause_until.lock().unwrap();
                    if automatic_due(
                        &s,
                        now.weekday().num_days_from_monday(),
                        now.hour() * 60 + now.minute(),
                        last.elapsed(),
                        deadline,
                        SystemTime::now(),
                    ) {
                        if let Err(e) = show(&handle, &s, true) {
                            eprintln!("예약 표시 실패: {e}");
                        }
                        last = Instant::now();
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        hide_main(window.app_handle());
                    }
                    #[cfg(target_os = "windows")]
                    tauri::WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                        hide_main(window.app_handle())
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_monitors,
            save_settings,
            preview,
            overlay_ready,
            overlay_done,
            quit_app
        ])
        .build(tauri::generate_context!())
        .expect("앱을 시작할 수 없습니다")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                restore_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_deadlines_replace_resume_and_expire() {
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        for minutes in [30, 60, 120, 240, 480] {
            let end = pause_deadline(now, minutes);
            assert!(paused(Some(end), end - Duration::from_secs(1)));
            assert!(!paused(Some(end), end));
        }
        let replaced = pause_deadline(now + Duration::from_secs(5), 60);
        assert!(paused(Some(replaced), pause_deadline(now, 30)));
        assert!(!paused(None, now));
        let mut settings = Settings::default();
        let elapsed = Duration::from_secs(100_000);
        let end = pause_deadline(now, 30);
        assert!(!automatic_due(&settings, 0, 600, elapsed, Some(end), now));
        assert!(automatic_due(&settings, 0, 600, elapsed, None, now));
        assert!(automatic_due(&settings, 0, 600, elapsed, Some(end), end));
        settings.enabled = false;
        assert!(!automatic_due(&settings, 0, 600, elapsed, Some(end), end));
    }
    #[test]
    fn tray_icon_has_transparent_background_and_visible_cross() {
        let icon = tray_icon_image();
        let pixels = icon.rgba();
        assert_eq!(pixels[3], 0);
        assert_eq!(pixels[(8 * 20 + 9) * 4 + 3], 255);
    }
    fn test_dir() -> std::path::PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("holypop-tests-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }
    fn fixture(path: &Path, data: &[u8]) {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap()
            .write_all(data)
            .unwrap();
    }
    #[test]
    fn appearance_migrates_and_validates() {
        let original = Settings::default();
        let mut json = serde_json::to_value(original).unwrap();
        json.as_object_mut().unwrap().remove("appearance");
        let mut migrated: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(migrated.appearance.style, "shadow");
        assert_eq!(migrated.appearance.width_percent, 70);
        assert!(validate(&migrated).is_ok());
        migrated.appearance.color = "#xx0000".into();
        assert!(validate(&migrated).is_err());
        migrated.appearance.color = "#112233".into();
        migrated.appearance.height_percent = 0;
        assert!(validate(&migrated).is_err());
        migrated.appearance.height_percent = 100;
        migrated.appearance.style = "bad".into();
        assert!(validate(&migrated).is_err());
    }
    #[test]
    fn monitor_selection_falls_back_after_disconnect() {
        let ids = vec!["left".into(), "right".into()];
        assert_eq!(select_monitors(&ids, "all", Some(1), Some(0)), vec![0, 1]);
        assert_eq!(select_monitors(&ids, "current", Some(1), Some(0)), vec![0]);
        assert_eq!(select_monitors(&ids, "left", Some(1), None), vec![0]);
        assert_eq!(select_monitors(&ids, "missing", Some(1), None), vec![1]);
        assert!(select_monitors(&[], "primary", None, None).is_empty());
    }
    #[test]
    fn normal_and_all_day_boundaries() {
        let mut s = Settings {
            weekdays: vec![0],
            ..Settings::default()
        };
        assert!(!active(&s, 0, 479));
        assert!(active(&s, 0, 480));
        assert!(active(&s, 0, 481));
        assert!(active(&s, 0, 1319));
        assert!(!active(&s, 0, 1320));
        assert!(!active(&s, 0, 1321));
        assert!(!active(&s, 1, 600));
        s.end_time = s.start_time.clone();
        assert!(active(&s, 0, 0));
        assert!(active(&s, 0, 1439));
        assert!(!active(&s, 1, 600));
    }
    #[test]
    fn interval_resets_prevent_duplicates_and_catch_up() {
        let mut s = Settings {
            interval_minutes: 1,
            weekdays: vec![0],
            ..Settings::default()
        };
        assert!(!due(&s, 0, 600, Duration::from_secs(59)));
        assert!(due(&s, 0, 600, Duration::from_secs(60)));
        assert!(!due(&s, 0, 600, Duration::ZERO));
        assert!(due(&s, 0, 600, Duration::from_secs(3600)));
        assert!(!due(&s, 0, 600, Duration::ZERO));
        assert!(!due(&s, 1, 600, Duration::from_secs(60)));
        assert!(!due(&s, 0, 0, Duration::from_secs(60)));
        s.enabled = false;
        assert!(!due(&s, 0, 600, Duration::from_secs(60)));
    }
    #[test]
    fn parses_strict_local_times() {
        assert_eq!(minutes("00:00"), Some(0));
        assert_eq!(minutes("23:59"), Some(1439));
        for bad in [
            "24:00", "12:60", "8:00", "08:0", "-1:00", "08:00:00", " 08:00", "aa:bb",
        ] {
            assert_eq!(minutes(bad), None, "{bad}");
        }
    }
    #[test]
    fn rejects_inactive_filter_and_invalid_custom_verses() {
        let mut s = Settings::default();
        s.weekdays.clear();
        assert!(validate(&s).is_err());
        s.enabled = false;
        assert!(validate(&s).is_ok());
        s.custom_verses.push(Verse {
            reference: "창세기 1:1".into(),
            text: "처음에".into(),
            version: "사용자".into(),
        });
        s.mode = "selected".into();
        s.version = "WEB".into();
        s.selected_index = 10;
        assert!(validate(&s).is_err());
        s.version = "사용자".into();
        assert!(validate(&s).is_ok());
        s.version = "없는 번역".into();
        assert!(validate(&s).is_err());
        s.version = "사용자".into();
        s.custom_verses[0].text = " ".into();
        assert!(validate(&s).is_err());
        s.custom_verses[0].text = "가".repeat(3334);
        assert!(validate(&s).is_err());
        s.custom_verses[0].text = "본문".into();
        s.custom_verses[0].reference = " ".into();
        assert!(validate(&s).is_err());
        s.custom_verses[0].reference = "창세기 1:1".into();
        s.custom_verses[0].version = " ".into();
        assert!(validate(&s).is_err());
        s.custom_verses[0].version = "v".repeat(101);
        assert!(validate(&s).is_err());
        s.custom_verses[0].version = "사용자".into();
        s.custom_verses[0].reference = "r".repeat(201);
        assert!(validate(&s).is_err());
    }
    #[test]
    fn persistence_roundtrip_and_corruption_preserve_files() {
        let dir = test_dir();
        let mut s = Settings::default();
        let original_text = "한글 \"따옴표\"\n<script>문자 그대로</script>";
        s.custom_verses.push(Verse {
            reference: "창세기 1:1".into(),
            text: original_text.into(),
            version: "개인".into(),
        });
        persist_settings(&dir, &s).unwrap();
        let (loaded, warning) = load_from_dir(&dir);
        assert!(warning.is_none());
        assert_eq!(loaded.custom_verses[0].text, original_text);
        let original = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let original_data = fs::read(&original).unwrap();
        let corrupt = dir.join("settings-999999999999999999999999999999999999999.json");
        fixture(&corrupt, b"{invalid json");
        let (recovered, warning) = load_from_dir(&dir);
        assert!(warning.is_some());
        assert_eq!(recovered.custom_verses[0].text, original_text);
        assert_eq!(fs::read(&original).unwrap(), original_data);
        assert_eq!(fs::read(&corrupt).unwrap(), b"{invalid json");
        assert!(persist_settings(&original, &s).is_err());
        assert_eq!(fs::read(&original).unwrap(), original_data);
        assert!(load_from_dir(&original).1.is_some());
        s.custom_verses = vec![
            Verse {
                reference: "A".into(),
                text: "x".repeat(10000),
                version: "V".into()
            };
            1000
        ];
        assert!(validate(&s).is_ok());
        assert!(persist_settings(&dir, &s).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }
    #[test]
    fn all_corrupt_settings_restore_defaults_with_warning() {
        let dir = test_dir();
        let path = dir.join("settings-1.json");
        fixture(&path, b"not json");
        let (s, warning) = load_from_dir(&dir);
        assert_eq!(s.interval_minutes, Settings::default().interval_minutes);
        assert!(warning.is_some());
        assert_eq!(fs::read(path).unwrap(), b"not json");
    }
    #[test]
    fn overnight_uses_start_day() {
        let mut s = Settings::default();
        s.start_time = "22:00".into();
        s.end_time = "02:00".into();
        s.weekdays = vec![0];
        assert!(active(&s, 0, 23 * 60));
        assert!(active(&s, 1, 60));
        assert!(!active(&s, 1, 22 * 60));
        assert!(!active(&s, 1, 120));
        s.weekdays = vec![6];
        assert!(active(&s, 6, 22 * 60));
        assert!(active(&s, 0, 119));
        assert!(!active(&s, 0, 120));
    }
    #[test]
    fn selection_respects_mode_and_version() {
        let mut s = Settings::default();
        s.custom_verses.push(Verse {
            reference: "A".into(),
            text: "B".into(),
            version: "Custom".into(),
        });
        s.version = "Custom".into();
        let list = verses(&s);
        assert_eq!(choose_index(&s, &list, 42, 0), 10);
        assert_eq!(choose_index(&s, &list, 42, 8), 10);
        assert_eq!(choose_index(&s, &list, 43, 0), 10);
        s.version = "WEB".into();
        assert_eq!(
            choose_index(&s, &list, 42, 0),
            choose_index(&s, &list, 42, 9)
        );
        assert_eq!(list[choose_index(&s, &list, 43, 0)].version, "WEB");
        s.version = "Custom".into();
        s.mode = "random".into();
        for random in 0..100 {
            assert_eq!(choose_index(&s, &list, 42, random), 10);
        }
        s.version = "WEB".into();
        for random in 0..100 {
            assert_eq!(list[choose_index(&s, &list, 42, random)].version, "WEB");
        }
        s.mode = "selected".into();
        s.selected_index = 2;
        s.version = "all".into();
        assert_eq!(choose_index(&s, &list, 42, 9), 2);
    }
    #[test]
    fn validates_boundaries() {
        let mut s = Settings::default();
        assert!(validate(&s).is_ok());
        s.start_time = "24:00".into();
        assert!(validate(&s).is_err());
        s.start_time = "08:00".into();
        s.selected_index = 10;
        assert!(validate(&s).is_err());
        s.selected_index = 0;
        for interval in [0, 1441] {
            s.interval_minutes = interval;
            assert!(validate(&s).is_err());
        }
        s.interval_minutes = 1;
        assert!(validate(&s).is_ok());
        let mut json = serde_json::to_value(&s).unwrap();
        json["interval_minutes"] = serde_json::json!(-1);
        assert!(serde_json::from_value::<Settings>(json).is_err());
    }
}
