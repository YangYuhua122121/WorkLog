//! workLog · 桌面端核心
//!
//! 双窗口：`clip`（悬浮球，小窗口常驻） + `board`（主面板，隐藏 / 显示）。
//! 悬浮球拖动时主面板跟随，展开 / 收起不再 resize 窗口。

mod commands;
mod config;
mod db;
mod export;
mod report;
mod stats;
mod timer;

use std::io::Write as _;
use std::path::PathBuf;

/// 单实例用的进程文件：放在应用数据目录，记录持锁进程的 pid。
fn instance_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))?
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    Some(base.join("com.worklog.desktop").join("instance.pid"))
}

/// 进程是否活着（不引第三方 crate）
fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            CloseHandle(handle);
            true
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn read_instance_pid() -> Option<u32> {
    let path = instance_file()?;
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// 已有活着的实例时返回它的 pid。
fn existing_instance_pid() -> Option<u32> {
    let pid = read_instance_pid()?;
    if pid == std::process::id() {
        return None;
    }
    pid_alive(pid).then_some(pid)
}

fn claim_instance() {
    let Some(path) = instance_file() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, std::process::id().to_string());
}

/// 启动日志：Windows 下没有控制台，安装后打不开时用户可以把这份日志发回来。
fn startup_log_path() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("com.worklog.desktop")
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Logs/workLog")
    } else {
        std::env::temp_dir().join("workLog")
    };
    base.join("startup.log")
}

fn startup_log(message: &str) {
    let path = startup_log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{} {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            message
        );
    }
}

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent, Wry};

/// 主面板相对某个悬浮球位置的目标坐标：
/// - 水平：悬浮球中心对齐主面板中心
/// - 垂直：主面板上边缘落在悬浮球中央水平线
pub(crate) fn board_position_for(
    app: &AppHandle,
    cp: PhysicalPosition<i32>,
) -> Result<(i32, i32), String> {
    let st = app.state::<UiState>();
    let (cw, _ch) = *st.clip_size.lock().map_err(|_| "窗口状态锁异常".to_string())?;
    let (bw, _bh) = *st.board_size.lock().map_err(|_| "窗口状态锁异常".to_string())?;
    let scale = *st.scale.lock().map_err(|_| "窗口状态锁异常".to_string())?;
    let x = cp.x + cw as i32 / 2 - bw as i32 / 2;
    // 垂直位置固定在「悬浮球第一行中心」：计时态变高时主面板不跟着上下跳
    let y = cp.y + (31.0 * scale).round() as i32;
    Ok((x, y))
}

/// 用悬浮球当前位置计算主面板目标位置
pub(crate) fn board_target_position(app: &AppHandle) -> Result<(i32, i32), String> {
    let clip = app
        .get_webview_window("clip")
        .ok_or_else(|| "悬浮球窗口不存在".to_string())?;
    let cp = clip.outer_position().map_err(|e| e.to_string())?;
    board_position_for(app, cp)
}

/// 让主面板跟随悬浮球
pub(crate) fn sync_board_position(app: &AppHandle) -> Result<(), String> {
    let (x, y) = board_target_position(app)?;
    let board = app
        .get_webview_window("board")
        .ok_or_else(|| "主面板窗口不存在".to_string())?;
    board
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("同步主面板位置失败：{e}"))?;
    Ok(())
}

/// 用已知的悬浮球位置同步主面板，避免拖动时每条 Moved 事件都做多次 WindowServer 查询。
pub(crate) fn sync_board_at(app: &AppHandle, cp: PhysicalPosition<i32>) -> Result<(), String> {
    let (x, y) = board_position_for(app, cp)?;
    let board = app
        .get_webview_window("board")
        .ok_or_else(|| "主面板窗口不存在".to_string())?;
    board
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("同步主面板位置失败：{e}"))?;
    Ok(())
}

pub(crate) fn set_clip_size(app: &AppHandle, width: u32, height: u32) {
    if let Ok(mut s) = app.state::<UiState>().clip_size.lock() {
        *s = (width, height);
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn board_height(app: &AppHandle) -> u32 {
    app.state::<UiState>()
        .board_size
        .lock()
        .map(|g| g.1)
        .unwrap_or(680)
}

fn board_is_visible(app: &AppHandle) -> bool {
    // 用缓存值，避免 60Hz 跟随线程反复查询 Win32 窗口状态
    app.state::<UiState>()
        .board_was_visible
        .lock()
        .map(|v| *v)
        .unwrap_or(false)
}

/// 拖动悬浮球时，把位置塞进通道，由单一跟随线程按 ~60Hz 取最新值同步主面板。
/// 这样 Moved 事件只做一次 send，不再每条都查窗口 / 调 SetWindowPos，也不会为节流收尾
/// 反复创建线程；拖动结束后最后一条位置一定会被同步。
#[cfg(not(target_os = "macos"))]
pub(crate) fn queue_board_follow(app: &AppHandle, cp: PhysicalPosition<i32>) {
    let st = app.state::<UiState>();
    if let Ok(tx) = st.board_follow_tx.lock() {
        if let Some(tx) = tx.as_ref() {
            let _ = tx.send(cp);
        }
    };
}

fn board_follow_loop(app: AppHandle, rx: std::sync::mpsc::Receiver<PhysicalPosition<i32>>) {
    while let Ok(mut pos) = rx.recv() {
        loop {
            while let Ok(p) = rx.try_recv() {
                pos = p;
            }
            if board_is_visible(&app) {
                let _ = sync_board_at(&app, pos);
            }
            std::thread::sleep(Duration::from_millis(16));
            let mut got_new = false;
            while let Ok(p) = rx.try_recv() {
                pos = p;
                got_new = true;
            }
            if !got_new {
                break;
            }
        }
    }
}

/// 悬浮球位置变化后延迟 450ms（松手后）通知前端保存一次位置，
/// 避免每个 Moved 事件都走一遍 Rust → WebView2 IPC。
fn schedule_clip_move_settle(app: &AppHandle) {
    let st = app.state::<UiState>();
    let start_watcher = match st.last_clip_move.lock() {
        Ok(mut last) => {
            let start = last.is_none();
            *last = Some(Instant::now());
            start
        }
        Err(_) => false,
    };
    if !start_watcher {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(180));
        let st = app.state::<UiState>();
        let last = st.last_clip_move.lock().ok().and_then(|g| *g);
        let Some(last) = last else { break };
        if last.elapsed() < Duration::from_millis(450) {
            continue;
        }
        if let Ok(mut l) = st.last_clip_move.lock() {
            *l = None;
        }
        let _ = app.emit("clip-moved-settled", ());
        break;
    });
}

/// macOS：让悬浮球窗口始终位于主面板之上（两者都在普通应用之上）
#[cfg(target_os = "macos")]
fn set_window_levels(app: &AppHandle) {
    use objc2_app_kit::{NSFloatingWindowLevel, NSWindow};
    if let Some(board) = app.get_webview_window("board") {
        if let Ok(ptr) = board.ns_window() {
            let win = unsafe { &*(ptr as *mut NSWindow) };
            win.setLevel(NSFloatingWindowLevel);
        }
    }
    if let Some(clip) = app.get_webview_window("clip") {
        if let Ok(ptr) = clip.ns_window() {
            let win = unsafe { &*(ptr as *mut NSWindow) };
            win.setLevel(NSFloatingWindowLevel + 1);
        }
    }
    link_board_to_clip(app);
}

/// macOS：把主面板作为悬浮球的子窗口，并排在悬浮球下方。
/// 这样拖动悬浮球时系统自动带着主面板走，不存在 IPC 跟随延迟。
#[cfg(target_os = "macos")]
pub(crate) fn link_board_to_clip(app: &AppHandle) {
    use objc2_app_kit::{NSWindow, NSWindowOrderingMode};
    let (Some(clip), Some(board)) = (
        app.get_webview_window("clip"),
        app.get_webview_window("board"),
    ) else {
        return;
    };
    if let (Ok(cp), Ok(bp)) = (clip.ns_window(), board.ns_window()) {
        let clip_win = unsafe { &*(cp as *mut NSWindow) };
        let board_win = unsafe { &*(bp as *mut NSWindow) };
        unsafe {
            clip_win.addChildWindow_ordered(board_win, NSWindowOrderingMode::Below);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unlink_board_from_clip(app: &AppHandle) {
    use objc2_app_kit::NSWindow;
    let (Some(clip), Some(board)) = (
        app.get_webview_window("clip"),
        app.get_webview_window("board"),
    ) else {
        return;
    };
    if let (Ok(cp), Ok(bp)) = (clip.ns_window(), board.ns_window()) {
        let clip_win = unsafe { &*(cp as *mut NSWindow) };
        let board_win = unsafe { &*(bp as *mut NSWindow) };
        clip_win.removeChildWindow(board_win);
    }
}

#[cfg(not(target_os = "macos"))]
fn set_window_levels(_app: &AppHandle) {}

/// 持有托盘对象：Tauri 的托盘在 drop 时会移除图标，必须保证生命周期与应用一致
struct TrayHandle(#[allow(dead_code)] tauri::tray::TrayIcon<Wry>);

/// 托盘隐藏 / 显示时记住主面板是否打开着
#[derive(Default)]
struct UiState {
    board_was_visible: Mutex<bool>,
    /// 悬浮球窗口尺寸（物理像素），拖动时复用，避免反复查询
    clip_size: Mutex<(u32, u32)>,
    /// 主面板窗口尺寸（物理像素）
    board_size: Mutex<(u32, u32)>,
    scale: Mutex<f64>,
    /// 主面板跟随：悬浮球位置通道（由单独线程按 ~60Hz 消费）
    board_follow_tx: Mutex<Option<std::sync::mpsc::Sender<PhysicalPosition<i32>>>>,
    /// 悬浮球最后一次移动时间，用于松手后只保存一次位置
    last_clip_move: Mutex<Option<Instant>>,
    /// 主面板原生滑动动画的代号：新动画/关闭时自增，旧动画线程自行退出
    #[allow(dead_code)]
    board_anim: Mutex<u64>,
    /// 悬浮球右键菜单（只创建一次，避免菜单句柄泄漏）
    clip_menu: Mutex<Option<Menu<Wry>>>,
}

/// Windows：主面板「从屏幕上方滑下来」的窗口位置动画。
/// 窗口本身不透明，所以不能靠 CSS 裁切/位移来做展开；移动窗口没有重绘，
/// 既不会出现整块面板突然闪现，也不会让面板里的文字进入合成层而发虚。
#[cfg(target_os = "windows")]
pub(crate) fn slide_board_window(
    app: AppHandle,
    from_y: i32,
    to_y: i32,
    hide_after: bool,
) {
    let gen = {
        let st = app.state::<UiState>();
        let value = match st.board_anim.lock() {
            Ok(mut g) => {
                *g += 1;
                *g
            }
            Err(_) => return,
        };
        value
    };
    std::thread::spawn(move || {
        let Some(board) = app.get_webview_window("board") else {
            return;
        };
        let x = board.outer_position().map(|p| p.x).unwrap_or(0);
        let current = |app: &AppHandle| {
            app.state::<UiState>()
                .board_anim
                .lock()
                .map(|g| *g)
                .unwrap_or(u64::MAX)
        };
        // 先让 WebView 在起始位置画出第一帧，再开始滑动，避免滑动时露出空白
        std::thread::sleep(Duration::from_millis(45));
        if current(&app) != gen {
            return;
        }
        let steps = 12u32;
        let step_sleep = Duration::from_millis(180) / steps;
        for i in 0..=steps {
            if current(&app) != gen {
                return;
            }
            let t = i as f64 / steps as f64;
            let eased = 1.0 - (1.0 - t).powi(3);
            let y = from_y + (((to_y - from_y) as f64) * eased).round() as i32;
            let _ = board.set_position(PhysicalPosition::new(x, y));
            if !hide_after && (i == 0 || i == steps / 2) {
                raise_clip(&app);
            }
            if i < steps {
                std::thread::sleep(step_sleep);
            }
        }
        if current(&app) != gen {
            return;
        }
        if hide_after {
            let _ = board.hide();
        } else {
            let _ = sync_board_position(&app);
            raise_clip(&app);
        }
    });
}

#[cfg(target_os = "windows")]
pub(crate) fn cancel_board_animation(app: &AppHandle) {
    if let Ok(mut g) = app.state::<UiState>().board_anim.lock() {
        *g += 1;
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn cancel_board_animation(_app: &AppHandle) {}

#[cfg(target_os = "windows")]
pub(crate) fn raise_clip_soon(app: &AppHandle, delay_ms: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));
        raise_clip(&app);
    });
}


/// Windows：让悬浮球始终压在主面板上面。
/// 两个窗口都是 topmost，单独把悬浮球设成 topmost 并不能改变 topmost 组内的顺序；
/// 这里直接调整两者的相对层级：主面板排在悬浮球之后（即悬浮球在上）。
#[cfg(target_os = "windows")]
pub(crate) fn raise_clip(app: &AppHandle) {
    use std::ffi::c_void;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let Some(clip) = app.get_webview_window("clip") else {
        return;
    };
    let Ok(clip_hwnd) = clip.hwnd() else {
        return;
    };
    let board_hwnd = app.get_webview_window("board").and_then(|b| b.hwnd().ok());
    unsafe {
        if let Some(board_hwnd) = board_hwnd {
            // 主面板排在悬浮球之后 = 悬浮球在上
            SetWindowPos(
                board_hwnd.0 as *mut c_void,
                clip_hwnd.0 as *mut c_void,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        // 悬浮球自身也保持在 topmost 组内
        SetWindowPos(
            clip_hwnd.0 as *mut c_void,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub(crate) fn set_board_visible(app: &AppHandle, visible: bool) {
    if let Ok(mut w) = app.state::<UiState>().board_was_visible.lock() {
        *w = visible;
    }
}

fn hide_all(app: &AppHandle) {
    cancel_board_animation(app);
    let state = app.state::<UiState>();
    if let Some(board) = app.get_webview_window("board") {
        if let Ok(mut w) = state.board_was_visible.lock() {
            *w = board.is_visible().unwrap_or(false);
        }
        let _ = board.hide();
    }
    if let Some(clip) = app.get_webview_window("clip") {
        let _ = clip.hide();
    }
}

fn show_all(app: &AppHandle) {
    cancel_board_animation(app);
    let state = app.state::<UiState>();
    let restore_board = state
        .board_was_visible
        .lock()
        .map(|g| *g)
        .unwrap_or(false);
    if let Some(clip) = app.get_webview_window("clip") {
        let _ = clip.show();
    }
    if restore_board {
        let _ = sync_board_position(app);
        if let Some(board) = app.get_webview_window("board") {
            let _ = board.show();
        }
        #[cfg(target_os = "macos")]
        link_board_to_clip(app);
        #[cfg(target_os = "windows")]
        raise_clip(app);
        // 托盘恢复时前端可能还以为面板关着 / 正在做收起动画，通知它回到展开态
        let _ = app.emit("board-restored", ());
    }
}

/// 悬浮球右键菜单。原生菜单可以弹出到小窗口之外，不受悬浮球 138×66 的尺寸限制。
fn build_clip_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let board = MenuItem::with_id(app, "clip:board", "展开 / 收起面板", true, None::<&str>)?;
    let timer = MenuItem::with_id(app, "clip:timer", "开始 / 停止计时", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "clip:settings", "打开设置", true, None::<&str>)?;
    let data = MenuItem::with_id(app, "clip:data", "打开数据目录", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "clip:hide", "隐藏到托盘", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "clip:quit", "退出 workLog", true, None::<&str>)?;
    let sep_a = PredefinedMenuItem::separator(app)?;
    let sep_b = PredefinedMenuItem::separator(app)?;
    let menu = Menu::new(app)?;
    menu.append(&board)?;
    menu.append(&timer)?;
    menu.append(&settings)?;
    menu.append(&data)?;
    menu.append(&sep_a)?;
    menu.append(&hide)?;
    menu.append(&sep_b)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn handle_clip_menu(app: &AppHandle, id: &str) {
    match id {
        "clip:board" => {
            let _ = app.emit("toggle-board", ());
        }
        "clip:timer" => {
            let _ = app.emit("clip-toggle-timer", ());
        }
        "clip:settings" => {
            // 面板窗口即使隐藏着，WebView 仍在运行；openSettings 会自己把面板打开
            let _ = app.emit("open-settings", ());
        }
        "clip:data" => {
            let _ = export::open_data_dir(app.clone());
        }
        "clip:hide" => hide_all(app),
        "clip:quit" => app.exit(0),
        _ => {}
    }
}

/// 托盘菜单：显示 / 隐藏、开始 / 停止、退出
/// Windows：把 WebView2 Runtime 的检测结果写进启动日志，方便排查「没有注册类」
#[cfg(target_os = "windows")]
fn log_webview2_status() {
    const KEY: &str = r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let out = std::process::Command::new("reg")
        .args(["query", &format!("HKLM\\{KEY}"), "/v", "pv"])
        .output();
    let detected = matches!(&out, Ok(o) if o.status.success());
    startup_log(&format!("webview2 registered: {detected}"));
    if !detected {
        startup_log(
            "webview2 runtime not found - install Microsoft Edge WebView2 Runtime (Evergreen) and retry",
        );
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "开始 / 停止计时", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("workLog")
        .menu(&menu)
        .on_menu_event(|tray, event| {
            let app = tray.app_handle();
            match event.id().as_ref() {
                "show" => {
                    let visible = app
                        .get_webview_window("clip")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    if visible {
                        hide_all(&app);
                    } else {
                        show_all(&app);
                    }
                }
                "toggle" => {
                    if let Some(clip) = app.get_webview_window("clip") {
                        let _ = clip.show();
                    }
                    let _ = app.emit("tray-toggle-timer", ());
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::TrayIconEvent;
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_all(&app);
            }
        });

    // macOS 用模板图标（系统自动适配深浅色），Windows 用彩色图标；
    // 托盘只在这里创建一次，配置文件里不再声明 trayIcon，避免 macOS 出现两个状态栏图标。
    #[cfg(target_os = "macos")]
    let tray_bytes = include_bytes!("../icons/tray-template.png");
    #[cfg(not(target_os = "macos"))]
    let tray_bytes = include_bytes!("../icons/tray-color.png");
    builder = builder.icon(tauri::image::Image::from_bytes(tray_bytes)?);
    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }
    let tray = builder.build(app)?;
    app.manage(TrayHandle(tray));
    startup_log("tray built");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        startup_log(&format!("panic: {info}"));
    }));
    startup_log(&format!("start v{}", env!("CARGO_PKG_VERSION")));

    tauri::Builder::default()
        // 单实例：第二次启动由 setup 里的 pid 检测直接退出；
        // 这里只处理“已运行实例被唤起”时把悬浮球 / 面板显示出来
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_all(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // 单实例：在创建窗口 / 托盘之前就判断，重复启动直接退出
            if let Some(prev) = existing_instance_pid() {
                startup_log(&format!("second instance detected (first pid {prev}), exiting"));
                // 先让第一个实例把悬浮球 / 面板显示出来，再退出
                show_all(&handle);
                std::thread::sleep(Duration::from_millis(300));
                std::process::exit(0);
            }
            claim_instance();
            startup_log(&format!("first instance pid {}", std::process::id()));

            config::init(&handle).map_err(std::io::Error::other)?;
            db::init(&handle).map_err(std::io::Error::other)?;
            app.manage(timer::TimerState::default());
            timer::restore(&handle);
            app.manage(UiState::default());

            // 主面板跟随线程：悬浮球 Moved → 通道 → 最多 60Hz 同步一次
            let (follow_tx, follow_rx) = std::sync::mpsc::channel();
            let follow_app = handle.clone();
            std::thread::spawn(move || board_follow_loop(follow_app, follow_rx));
            if let Ok(mut slot) = handle.state::<UiState>().board_follow_tx.lock() {
                *slot = Some(follow_tx);
            }

            // 右键菜单只创建一次；菜单弹出时复用，避免句柄堆积
            let clip_menu = build_clip_menu(&handle)?;
            if let Ok(mut slot) = handle.state::<UiState>().clip_menu.lock() {
                *slot = Some(clip_menu);
            }

            #[cfg(target_os = "windows")]
            log_webview2_status();

            set_window_levels(&handle);

            // 缓存两窗尺寸 / 缩放，拖动同步时不再逐次查询
            {
                let st = handle.state::<UiState>();
                if let Some(clip) = app.get_webview_window("clip") {
                    if let Ok(sz) = clip.outer_size() {
                        *st.clip_size.lock().unwrap() = (sz.width, sz.height);
                    }
                    if let Ok(sc) = clip.scale_factor() {
                        *st.scale.lock().unwrap() = sc;
                    }
                }
                if let Some(board) = app.get_webview_window("board") {
                    if let Ok(sz) = board.outer_size() {
                        *st.board_size.lock().unwrap() = (sz.width, sz.height);
                    }
                }
            }

            // 还原悬浮球上次位置，并把主面板摆到正确位置
            let cfg = config::load(&handle);
            if cfg.window.saved {
                if let Some(clip) = app.get_webview_window("clip") {
                    let _ = clip.set_position(PhysicalPosition::new(cfg.window.x, cfg.window.y));
                }
            }
            let _ = sync_board_position(&handle);
            // 挂成 child window 会让隐藏的主面板变成可显示状态，这里重新收回
            if let Some(board) = app.get_webview_window("board") {
                let _ = board.hide();
            }

            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    // 关闭窗口不退出进程（常驻托盘）
                    api.prevent_close();
                    if label == "clip" {
                        hide_all(window.app_handle());
                    } else {
                        cancel_board_animation(window.app_handle());
                        let _ = window.hide();
                        set_board_visible(window.app_handle(), false);
                        let _ = window.app_handle().emit("board-force-closed", ());
                    }
                }
                WindowEvent::Moved(pos) if label == "clip" => {
                    let app = window.app_handle();
                    #[cfg(not(target_os = "macos"))]
                    queue_board_follow(app, *pos);
                    // 松手后才通知前端保存一次位置，避免拖动期间每条 Moved 都走 WebView IPC
                    schedule_clip_move_settle(app);
                }
                WindowEvent::Focused(true) if label == "board" => {
                    // 点击主面板会让它被系统提到 topmost 组的最前，把悬浮球重新压回去
                    #[cfg(target_os = "windows")]
                    raise_clip(window.app_handle());
                }
                WindowEvent::Resized(size) if label == "clip" => {
                    set_clip_size(window.app_handle(), size.width, size.height);
                    if board_is_visible(window.app_handle()) {
                        let _ = sync_board_position(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id.starts_with("clip:") {
                handle_clip_menu(app, id);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::get_task,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::list_stages,
            commands::add_stage,
            commands::update_stage,
            commands::delete_stage,
            commands::start_timer,
            commands::stop_timer,
            commands::discard_pending,
            commands::timer_state,
            commands::keep_working,
            commands::get_summary,
            commands::generate_report,
            commands::ai_rewrite_report,
            commands::reset_report_config,
            commands::get_config,
            commands::save_config,
            commands::save_window_pos,
            commands::show_board,
            commands::hide_board,
            commands::show_clip_menu,
            commands::resize_clip_window,
            commands::open_config_file,
            commands::app_info,
            export::export_tasks,
            export::open_data_dir,
        ])
        .run(tauri::generate_context!())
        .expect("workLog 启动失败");
}
