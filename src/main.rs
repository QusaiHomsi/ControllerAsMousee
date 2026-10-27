#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Stroke, Vec2};
use gilrs::{Axis, Button as PadButton, GamepadId, Gilrs};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
#[cfg(windows)]
use vigem_rust::{Client as VigemClient, TargetHandle, X360Button, X360Report, Xbox360};
use winreg::{RegKey, enums::*};

const APP_NAME: &str = "ControllerToPC";
const APP_TITLE: &str = "Controller Control";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const TABLET_TIP_KEY: &str = r"Software\Microsoft\TabletTip\1.7";
const TOUCH_KEYBOARD_VALUE: &str = "EnableDesktopModeAutoInvoke";

#[cfg(windows)]
static TOUCH_KEYBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static TOUCH_KEYBOARD_LAST_LAUNCH: AtomicU64 = AtomicU64::new(0);
#[cfg(windows)]
static TOUCH_KEYBOARD_WATCHER_STARTED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static TOUCH_KEYBOARD_CLOCK: OnceLock<Instant> = OnceLock::new();

const BG: Color32 = Color32::from_rgb(16, 25, 29);
const PANEL: Color32 = Color32::from_rgb(25, 39, 43);
const PANEL_HI: Color32 = Color32::from_rgb(35, 54, 58);
const EDGE: Color32 = Color32::from_rgb(48, 70, 74);
const INK: Color32 = Color32::from_rgb(239, 245, 239);
const MUTED: Color32 = Color32::from_rgb(154, 173, 170);
const MINT: Color32 = Color32::from_rgb(101, 216, 189);
const MINT_DIM: Color32 = Color32::from_rgb(36, 75, 69);
const FIELD: Color32 = Color32::from_rgb(17, 29, 32);
const RED: Color32 = Color32::from_rgb(255, 128, 115);

const ACTIONS: &[(&str, &str)] = &[
    ("none", "Do nothing"),
    ("mouse:left", "Left click"),
    ("mouse:left_hold", "Left click (hold to drag)"),
    ("mouse:right", "Right click"),
    ("mouse:middle", "Middle click"),
    ("mouse:double", "Double click"),
    ("media:playpause", "Play / Pause"),
    ("media:next", "Next track"),
    ("media:prev", "Previous track"),
    ("media:volup", "Volume up"),
    ("media:voldown", "Volume down"),
    ("media:mute", "Mute"),
    ("scroll:up", "Scroll up"),
    ("scroll:down", "Scroll down"),
    ("precision", "Slow cursor (hold)"),
    ("boost", "Fast cursor (hold)"),
    ("app:toggle", "Turn controller control on / off"),
    ("key", "Keyboard shortcut…"),
];

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Binding {
    #[serde(default = "default_input")]
    input: String,
    #[serde(default = "default_action")]
    action: String,
    #[serde(default)]
    key: String,
}

fn default_input() -> String {
    "none".into()
}
fn default_action() -> String {
    "none".into()
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    version: u32,
    enabled: bool,
    device: usize,
    mouse_stick: String,
    scroll_stick: String,
    mouse_speed: f32,
    curve: f32,
    deadzone: f32,
    invert_mouse_y: bool,
    scroll_speed: f32,
    invert_scroll: bool,
    precision_factor: f32,
    boost_factor: f32,
    repeat_delay: f32,
    repeat_rate: f32,
    keep_running: bool,
    xbox_output: bool,
    touch_keyboard: bool,
    // -1 means the registry value did not exist before we enabled it.
    touch_keyboard_previous: Option<i64>,
    bindings: Vec<Binding>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 3,
            enabled: true,
            device: 0,
            mouse_stick: "left".into(),
            scroll_stick: "right".into(),
            mouse_speed: 1200.0,
            curve: 1.5,
            deadzone: 0.15,
            invert_mouse_y: false,
            scroll_speed: 12.0,
            invert_scroll: false,
            precision_factor: 0.3,
            boost_factor: 2.0,
            repeat_delay: 0.4,
            repeat_rate: 0.08,
            keep_running: true,
            xbox_output: true,
            touch_keyboard: true,
            touch_keyboard_previous: None,
            bindings: vec![
                Binding {
                    input: "btn:0".into(),
                    action: "mouse:left_hold".into(),
                    key: String::new(),
                },
                Binding {
                    input: "btn:1".into(),
                    action: "mouse:right".into(),
                    key: String::new(),
                },
                Binding {
                    input: "btn:2".into(),
                    action: "media:playpause".into(),
                    key: String::new(),
                },
                Binding {
                    input: "btn:4".into(),
                    action: "media:prev".into(),
                    key: String::new(),
                },
                Binding {
                    input: "btn:5".into(),
                    action: "media:next".into(),
                    key: String::new(),
                },
                Binding {
                    input: "hat:up".into(),
                    action: "media:volup".into(),
                    key: String::new(),
                },
                Binding {
                    input: "hat:down".into(),
                    action: "media:voldown".into(),
                    key: String::new(),
                },
                Binding {
                    input: "hat:left".into(),
                    action: "key".into(),
                    key: "left".into(),
                },
                Binding {
                    input: "hat:right".into(),
                    action: "key".into(),
                    key: "right".into(),
                },
                Binding {
                    input: "axis+:4".into(),
                    action: "precision".into(),
                    key: String::new(),
                },
                Binding {
                    input: "axis+:5".into(),
                    action: "boost".into(),
                    key: String::new(),
                },
                Binding {
                    input: "btn:7".into(),
                    action: "app:toggle".into(),
                    key: String::new(),
                },
            ],
        }
    }
}

impl Config {
    fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        base.join(APP_NAME).join("config.json")
    }

    fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(|mut cfg| {
                if cfg.version < 3 {
                    // Enable the requested system-wide touch keyboard behavior
                    // for existing installs as well as fresh configurations.
                    cfg.touch_keyboard = true;
                    cfg.version = 3;
                }
                for binding in &mut cfg.bindings {
                    if !ACTIONS.iter().any(|(id, _)| *id == binding.action) {
                        binding.action = "none".into();
                    }
                }
                cfg
            })
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
        }
    }
}

fn action_label(action: &str) -> &'static str {
    ACTIONS
        .iter()
        .find(|(id, _)| *id == action)
        .map(|(_, name)| *name)
        .unwrap_or("Do nothing")
}

fn input_label(input: &str) -> String {
    if input == "none" {
        return "Not set".into();
    }
    if let Some(v) = input.strip_prefix("btn:") {
        return match v.parse::<usize>().unwrap_or(99) {
            0 => "A".into(),
            1 => "B".into(),
            2 => "X".into(),
            3 => "Y".into(),
            4 => "LB".into(),
            5 => "RB".into(),
            6 => "Back".into(),
            7 => "Start".into(),
            8 => "L3 (stick click)".into(),
            9 => "R3 (stick click)".into(),
            n => format!("Button {n}"),
        };
    }
    if let Some(v) = input.strip_prefix("hat:") {
        return format!("D-pad {}", capitalize(v));
    }
    if let Some(v) = input.strip_prefix("axis+:") {
        return match v {
            "4" => "LT (left trigger)".into(),
            "5" => "RT (right trigger)".into(),
            _ => format!("Axis {v}"),
        };
    }
    input.into()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

fn set_startup(enable: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(RUN_KEY).map_err(|e| e.to_string())?;
    if enable {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        run.set_value(APP_NAME, &format!("\"{}\" --minimized", exe.display()))
            .map_err(|e| e.to_string())
    } else {
        match run.delete_value(APP_NAME) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn startup_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|key| key.get_value::<String, _>(APP_NAME))
        .is_ok()
}

fn set_touch_keyboard(config: &mut Config, enabled: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let current_key = hkcu.open_subkey_with_flags(TABLET_TIP_KEY, KEY_READ | KEY_WRITE);
    let original = current_key
        .as_ref()
        .ok()
        .and_then(|key| key.get_value::<u32, _>(TOUCH_KEYBOARD_VALUE).ok());
    let (key, _) = hkcu
        .create_subkey(TABLET_TIP_KEY)
        .map_err(|e| e.to_string())?;

    if enabled {
        if config.touch_keyboard_previous.is_none() {
            config.touch_keyboard_previous = Some(original.map(i64::from).unwrap_or(-1));
        }
        key.set_value(TOUCH_KEYBOARD_VALUE, &1u32)
            .map_err(|e| e.to_string())?;
        config.touch_keyboard = true;
        #[cfg(windows)]
        TOUCH_KEYBOARD_ACTIVE.store(true, Ordering::Relaxed);
        broadcast_setting_change();
    } else {
        if let Some(previous) = config.touch_keyboard_previous.take() {
            if previous < 0 {
                let _ = key.delete_value(TOUCH_KEYBOARD_VALUE);
            } else {
                key.set_value(TOUCH_KEYBOARD_VALUE, &(previous as u32))
                    .map_err(|e| e.to_string())?;
            }
        }
        config.touch_keyboard = false;
        #[cfg(windows)]
        TOUCH_KEYBOARD_ACTIVE.store(false, Ordering::Relaxed);
        broadcast_setting_change();
    }
    config.save();
    Ok(())
}

#[cfg(windows)]
fn start_global_text_focus_watcher() {
    if TOUCH_KEYBOARD_WATCHER_STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::Builder::new()
        .name("touch-keyboard-focus".into())
        .spawn(global_text_focus_watcher)
        .ok();
}

#[cfg(windows)]
#[repr(C)]
struct MsaaVariant {
    vt: u16,
    reserved1: u16,
    reserved2: u16,
    reserved3: u16,
    // VARIANT is 16 bytes on 64-bit Windows: an 8-byte header and an
    // 8-byte value union. Keeping this exact size is required by COM ABI.
    data: [usize; 1],
}

#[cfg(windows)]
#[repr(C)]
struct WinMsg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    point: [i32; 2],
    private: u32,
}

#[cfg(windows)]
type WinEventCallback = unsafe extern "system" fn(isize, u32, isize, i32, i32, u32, u32);

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn SetWinEventHook(
        event_min: u32,
        event_max: u32,
        module: isize,
        callback: Option<WinEventCallback>,
        process_id: u32,
        thread_id: u32,
        flags: u32,
    ) -> isize;
    fn GetMessageW(message: *mut WinMsg, window: isize, min: u32, max: u32) -> i32;
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
    fn IsWindowVisible(window: isize) -> i32;
}

#[cfg(windows)]
#[link(name = "oleacc")]
unsafe extern "system" {
    fn AccessibleObjectFromEvent(
        window: isize,
        object_id: u32,
        child_id: u32,
        accessible: *mut *mut std::ffi::c_void,
        child: *mut MsaaVariant,
    ) -> i32;
}

#[cfg(windows)]
fn global_text_focus_watcher() {
    use std::ffi::c_void;

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, apartment: u32) -> i32;
        fn CoUninitialize();
    }

    const EVENT_OBJECT_FOCUS: u32 = 0x8005;
    // The hook callback runs on this thread, so initialize COM before registering it.
    let com_result = unsafe { CoInitializeEx(std::ptr::null_mut(), 2) };
    if com_result < 0 {
        return;
    }
    let hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_FOCUS,
            EVENT_OBJECT_FOCUS,
            0,
            Some(on_accessible_focus),
            0,
            0,
            0,
        )
    };
    if hook != 0 {
        let mut message = WinMsg {
            hwnd: 0,
            message: 0,
            w_param: 0,
            l_param: 0,
            time: 0,
            point: [0; 2],
            private: 0,
        };
        while unsafe { GetMessageW(&mut message, 0, 0, 0) } > 0 {}
    }
    unsafe { CoUninitialize() };
}

#[cfg(windows)]
unsafe extern "system" fn on_accessible_focus(
    _hook: isize,
    event: u32,
    window: isize,
    object_id: i32,
    child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    use std::ffi::c_void;

    if event != 0x8005
        || window == 0
        || !TOUCH_KEYBOARD_ACTIVE.load(Ordering::Relaxed)
        || !matches!(object_id, 0 | -4)
    {
        return;
    }

    let mut accessible = std::ptr::null_mut();
    let mut child = MsaaVariant {
        vt: 3, // VT_I4
        reserved1: 0,
        reserved2: 0,
        reserved3: 0,
        data: [child_id.max(0) as usize],
    };
    let result = unsafe {
        AccessibleObjectFromEvent(
            window,
            object_id as u32,
            child_id.max(0) as u32,
            &mut accessible,
            &mut child,
        )
    };
    if result < 0 || accessible.is_null() {
        return;
    }

    let mut role = MsaaVariant {
        vt: 0,
        reserved1: 0,
        reserved2: 0,
        reserved3: 0,
        data: [0; 1],
    };
    let vtable = unsafe { *(accessible as *mut *mut *mut c_void) };
    // IAccessible::get_accRole follows the seven IUnknown/IDispatch entries,
    // then get_accParent, get_accChildCount, get_accChild, get_accName, get_accValue,
    // and get_accDescription.
    let get_role: unsafe extern "system" fn(*mut c_void, MsaaVariant, *mut MsaaVariant) -> i32 =
        unsafe { std::mem::transmute(*vtable.add(13)) };
    let role_result = unsafe { get_role(accessible, child, &mut role) };
    let release: unsafe extern "system" fn(*mut c_void) -> u32 =
        unsafe { std::mem::transmute(*vtable.add(2)) };
    unsafe { release(accessible) };

    // ROLE_SYSTEM_TEXT, ROLE_SYSTEM_COMBOBOX, and ROLE_SYSTEM_DOCUMENT cover
    // standard edit fields, editable combos, and browser contenteditable areas.
    if role_result >= 0 && role.vt == 3 && matches!(role.data[0] as i32, 0x2a | 0x2e | 0x0f) {
        show_windows_touch_keyboard();
    }
}

#[cfg(windows)]
fn show_windows_touch_keyboard() {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let now = TOUCH_KEYBOARD_CLOCK
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis() as u64;
    let previous = TOUCH_KEYBOARD_LAST_LAUNCH.load(Ordering::Relaxed);
    if previous != 0 && now.saturating_sub(previous) < 300 {
        return;
    }
    let class: Vec<u16> = "IPTip_Main_Window".encode_utf16().chain([0]).collect();
    let keyboard_window = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if keyboard_window != 0 && unsafe { IsWindowVisible(keyboard_window) } != 0 {
        return;
    }
    let mut candidates = Vec::new();
    if let Some(common) = std::env::var_os("CommonProgramFiles") {
        candidates.push(std::path::PathBuf::from(common).join("Microsoft Shared\\Ink\\TabTip.exe"));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            std::path::PathBuf::from(program_files)
                .join("Common Files\\Microsoft Shared\\Ink\\TabTip.exe"),
        );
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(
            std::path::PathBuf::from(program_files_x86)
                .join("Common Files\\Microsoft Shared\\Ink\\TabTip.exe"),
        );
    }
    if let Some(path) = candidates.into_iter().find(|path| path.is_file())
        && Command::new(path)
            .creation_flags(0x08000000)
            .spawn()
            .is_ok()
    {
        TOUCH_KEYBOARD_LAST_LAUNCH.store(now.max(1), Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn broadcast_setting_change() {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageTimeoutW(
            hwnd: isize,
            msg: u32,
            wparam: usize,
            lparam: *const u16,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> isize;
    }
    let setting: Vec<u16> = TABLET_TIP_KEY
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            0xffff,
            0x001a,
            0,
            setting.as_ptr(),
            0x0002,
            1000,
            &mut result,
        );
    }
}

#[cfg(not(windows))]
fn broadcast_setting_change() {}

#[derive(Default)]
struct ButtonState {
    down: bool,
    started: f64,
    last_repeat: f64,
}

struct Controller {
    gilrs: Option<Gilrs>,
    last_init_attempt: Instant,
    connected: bool,
    active_id: Option<GamepadId>,
    name: String,
    devices: Vec<(GamepadId, String)>,
    sticks: [f32; 4],
    pressed: HashSet<String>,
    previous: HashSet<String>,
    binding_states: Vec<ButtonState>,
    remainder: [f32; 2],
    scroll_remainder: f32,
    left_held: bool,
    learning: Option<usize>,
    error: String,
    xbox_error: String,
    #[cfg(windows)]
    xbox_client: Option<VigemClient>,
    #[cfg(windows)]
    xbox_pad: Option<TargetHandle<Xbox360>>,
    last_xbox_attempt: Option<Instant>,
}

impl Controller {
    fn new() -> Self {
        let (gilrs, error) = match Gilrs::new() {
            Ok(gilrs) => (Some(gilrs), String::new()),
            Err(e) => {
                eprintln!("Controller input could not initialize: {e}");
                (None, e.to_string())
            }
        };
        Self {
            gilrs,
            last_init_attempt: Instant::now(),
            connected: false,
            active_id: None,
            name: String::new(),
            devices: Vec::new(),
            sticks: [0.0; 4],
            pressed: HashSet::new(),
            previous: HashSet::new(),
            binding_states: Vec::new(),
            remainder: [0.0; 2],
            scroll_remainder: 0.0,
            left_held: false,
            learning: None,
            error,
            xbox_error: String::new(),
            #[cfg(windows)]
            xbox_client: None,
            #[cfg(windows)]
            xbox_pad: None,
            last_xbox_attempt: None,
        }
    }

    #[cfg(windows)]
    fn update_xbox_output(
        &mut self,
        enabled: bool,
        buttons: [bool; 14],
        left_trigger: f32,
        right_trigger: f32,
        sticks: [f32; 4],
    ) {
        if !enabled {
            self.xbox_pad = None;
            self.xbox_client = None;
            self.xbox_error.clear();
            return;
        }
        if self.xbox_pad.is_none()
            && self
                .last_xbox_attempt
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(3))
        {
            self.last_xbox_attempt = Some(Instant::now());
            match VigemClient::connect() {
                Ok(client) => match client.new_x360_target().plug() {
                    Ok(pending) => {
                        self.xbox_pad = Some(pending.assume_ready());
                        self.xbox_client = Some(client);
                        self.xbox_error.clear();
                    }
                    Err(error) => self.xbox_error = format!("Xbox output unavailable: {error}"),
                },
                Err(error) => {
                    self.xbox_error = format!("Xbox output unavailable: install ViGEmBus ({error})")
                }
            }
        }
        let Some(pad) = &self.xbox_pad else {
            return;
        };
        let mut report = X360Report::default();
        for (is_down, button) in buttons.into_iter().zip([
            X360Button::A,
            X360Button::B,
            X360Button::X,
            X360Button::Y,
            X360Button::LEFT_SHOULDER,
            X360Button::RIGHT_SHOULDER,
            X360Button::BACK,
            X360Button::START,
            X360Button::LEFT_THUMB,
            X360Button::RIGHT_THUMB,
            X360Button::DPAD_UP,
            X360Button::DPAD_DOWN,
            X360Button::DPAD_LEFT,
            X360Button::DPAD_RIGHT,
        ]) {
            if is_down {
                report.buttons |= button;
            }
        }
        report.left_trigger = (left_trigger * 255.0).round() as u8;
        report.right_trigger = (right_trigger * 255.0).round() as u8;
        report.thumb_lx = (sticks[0] * i16::MAX as f32).round() as i16;
        report.thumb_ly = (sticks[1] * i16::MAX as f32).round() as i16;
        report.thumb_rx = (sticks[2] * i16::MAX as f32).round() as i16;
        report.thumb_ry = (sticks[3] * i16::MAX as f32).round() as i16;
        if let Err(error) = pad.update(&report) {
            self.xbox_error = format!("Xbox output stopped: {error}");
            self.xbox_pad = None;
            self.xbox_client = None;
        }
    }

    fn poll(&mut self, config: &mut Config, dt: f32, now: f64) {
        if self.gilrs.is_none() {
            if self.last_init_attempt.elapsed() < Duration::from_secs(3) {
                return;
            }
            self.last_init_attempt = Instant::now();
            match Gilrs::new() {
                Ok(gilrs) => {
                    self.gilrs = Some(gilrs);
                    self.error.clear();
                }
                Err(error) => {
                    self.error = error.to_string();
                    return;
                }
            }
        }
        self.devices = {
            let gilrs = self.gilrs.as_mut().expect("Gilrs was checked above");
            while gilrs.next_event().is_some() {}
            gilrs
                .gamepads()
                .map(|(id, pad)| (id, pad.name().to_owned()))
                .collect()
        };
        if !self.devices.is_empty() && config.device >= self.devices.len() {
            config.device = 0;
        }
        let Some((id, _)) = self.devices.get(config.device).cloned() else {
            if self.connected || self.active_id.is_some() {
                self.release_all();
            }
            self.active_id = None;
            self.connected = false;
            self.name.clear();
            self.sticks = [0.0; 4];
            self.pressed.clear();
            self.previous.clear();
            #[cfg(windows)]
            {
                self.xbox_pad = None;
                self.xbox_client = None;
            }
            return;
        };

        if self.active_id != Some(id) {
            self.release_all();
            self.previous.clear();
            self.pressed.clear();
            self.active_id = Some(id);
        }

        let gilrs = self.gilrs.as_mut().expect("Gilrs was checked above");
        let pad = gilrs.gamepad(id);
        self.connected = pad.is_connected();
        self.name = pad.name().to_owned();
        self.sticks = [
            pad.value(Axis::LeftStickX),
            pad.value(Axis::LeftStickY),
            pad.value(Axis::RightStickX),
            pad.value(Axis::RightStickY),
        ];

        let mut pressed = HashSet::new();
        let buttons = [
            PadButton::South,
            PadButton::East,
            PadButton::West,
            PadButton::North,
            PadButton::LeftTrigger,
            PadButton::RightTrigger,
            PadButton::Select,
            PadButton::Start,
            PadButton::LeftThumb,
            PadButton::RightThumb,
        ];
        for (index, button) in buttons.iter().enumerate() {
            if pad.is_pressed(*button) {
                pressed.insert(format!("btn:{index}"));
            }
        }
        for (direction, button) in [
            ("up", PadButton::DPadUp),
            ("down", PadButton::DPadDown),
            ("left", PadButton::DPadLeft),
            ("right", PadButton::DPadRight),
        ] {
            if pad.is_pressed(button) {
                pressed.insert(format!("hat:{direction}"));
            }
        }
        let left_trigger = pad
            .value(Axis::LeftZ)
            .max(
                pad.button_data(PadButton::LeftTrigger2)
                    .map_or(0.0, |value| value.value()),
            )
            .clamp(0.0, 1.0);
        let right_trigger = pad
            .value(Axis::RightZ)
            .max(
                pad.button_data(PadButton::RightTrigger2)
                    .map_or(0.0, |value| value.value()),
            )
            .clamp(0.0, 1.0);

        #[cfg(windows)]
        let xbox_buttons = [
            pad.is_pressed(PadButton::South),
            pad.is_pressed(PadButton::East),
            pad.is_pressed(PadButton::West),
            pad.is_pressed(PadButton::North),
            pad.is_pressed(PadButton::LeftTrigger),
            pad.is_pressed(PadButton::RightTrigger),
            pad.is_pressed(PadButton::Select),
            pad.is_pressed(PadButton::Start),
            pad.is_pressed(PadButton::LeftThumb),
            pad.is_pressed(PadButton::RightThumb),
            pad.is_pressed(PadButton::DPadUp),
            pad.is_pressed(PadButton::DPadDown),
            pad.is_pressed(PadButton::DPadLeft),
            pad.is_pressed(PadButton::DPadRight),
        ];
        #[cfg(windows)]
        self.update_xbox_output(
            config.xbox_output,
            xbox_buttons,
            left_trigger,
            right_trigger,
            self.sticks,
        );
        if left_trigger > 0.5 {
            pressed.insert("axis+:4".into());
        }
        if right_trigger > 0.5 {
            pressed.insert("axis+:5".into());
        }
        let new_inputs: Vec<String> = pressed.difference(&self.previous).cloned().collect();
        self.previous = pressed.clone();
        self.pressed = pressed;
        if let Some(index) = self.learning.take() {
            if let Some(input) = new_inputs.first() {
                if let Some(binding) = config.bindings.get_mut(index) {
                    binding.input.clone_from(input);
                }
                config.save();
            } else {
                self.learning = Some(index);
            }
            return;
        }

        if self.binding_states.len() != config.bindings.len() {
            if self.left_held {
                mouse_button(0x0004);
                self.left_held = false;
            }
            self.binding_states
                .resize_with(config.bindings.len(), ButtonState::default);
        }
        let mut precision = false;
        let mut boost = false;
        let mut scroll_dir = 0i32;
        let mut turned_off = false;
        for (index, binding) in config.bindings.iter().enumerate() {
            let is_down = binding.input != "none" && self.pressed.contains(&binding.input);
            let state = &mut self.binding_states[index];
            if binding.action == "none" || binding.input == "none" {
                state.down = is_down;
                continue;
            }
            if !config.enabled && binding.action != "app:toggle" {
                state.down = is_down;
                continue;
            }

            if is_down && !state.down {
                state.down = true;
                state.started = now;
                state.last_repeat = now;
                match binding.action.as_str() {
                    "mouse:left_hold" => {
                        mouse_button(0x0002);
                        self.left_held = true;
                    }
                    "app:toggle" => {
                        config.enabled = !config.enabled;
                        turned_off = !config.enabled;
                    }
                    "precision" | "boost" | "scroll:up" | "scroll:down" => {}
                    action => tap_action(action, &binding.key),
                }
            } else if is_down && state.down {
                let repeats = binding.action == "key"
                    || binding.action == "media:volup"
                    || binding.action == "media:voldown";
                if repeats
                    && now - state.started > config.repeat_delay as f64
                    && now - state.last_repeat > config.repeat_rate as f64
                {
                    state.last_repeat = now;
                    tap_action(&binding.action, &binding.key);
                }
            } else if !is_down && state.down {
                state.down = false;
                if binding.action == "mouse:left_hold" {
                    mouse_button(0x0004);
                    self.left_held = false;
                }
            }
            if state.down {
                match binding.action.as_str() {
                    "precision" => precision = true,
                    "boost" => boost = true,
                    "scroll:up" => scroll_dir += 1,
                    "scroll:down" => scroll_dir -= 1,
                    _ => {}
                }
            }
        }

        if turned_off {
            self.release_all();
            for (index, binding) in config.bindings.iter().enumerate() {
                if binding.action == "app:toggle" && self.pressed.contains(&binding.input) {
                    self.binding_states[index].down = true;
                }
            }
        }

        if !config.enabled {
            return;
        }
        let mouse_stick = match config.mouse_stick.as_str() {
            "right" => 2,
            "left" => 0,
            _ => 4,
        };
        if mouse_stick < 4 {
            let (x, mut y) = shape(
                self.sticks[mouse_stick],
                self.sticks[mouse_stick + 1],
                config.deadzone,
                config.curve,
            );
            // Preserve the controller's reported vertical direction by default.
            // Users can reverse it explicitly with the setting below.
            if config.invert_mouse_y {
                y = -y;
            }
            let mode = if precision {
                config.precision_factor
            } else {
                1.0
            } * if boost { config.boost_factor } else { 1.0 };
            self.remainder[0] += x * config.mouse_speed * mode * dt;
            self.remainder[1] += y * config.mouse_speed * mode * dt;
            let dx = self.remainder[0] as i32;
            let dy = self.remainder[1] as i32;
            if dx != 0 || dy != 0 {
                self.remainder[0] -= dx as f32;
                self.remainder[1] -= dy as f32;
                move_cursor_by(dx, dy);
            }
        }

        let mut notches = scroll_dir as f32 * config.scroll_speed * dt;
        let scroll_stick = match config.scroll_stick.as_str() {
            "right" => 2,
            "left" => 0,
            _ => 4,
        };
        if scroll_stick < 4 {
            let (_, y) = shape(
                self.sticks[scroll_stick],
                self.sticks[scroll_stick + 1],
                config.deadzone,
                config.curve,
            );
            notches -= y * config.scroll_speed * dt;
        }
        if config.invert_scroll {
            notches = -notches;
        }
        self.scroll_remainder += notches;
        let steps = self.scroll_remainder as i32;
        if steps != 0 {
            self.scroll_remainder -= steps as f32;
            mouse_wheel(steps * 120);
        }
    }

    fn release_all(&mut self) {
        if self.left_held {
            mouse_button(0x0004);
            self.left_held = false;
        }
        for state in &mut self.binding_states {
            state.down = false;
        }
    }
}

fn shape(x: f32, y: f32, deadzone: f32, curve: f32) -> (f32, f32) {
    let dz = deadzone.clamp(0.0, 0.9);
    let mag = x.hypot(y);
    if mag < dz || mag == 0.0 {
        return (0.0, 0.0);
    }
    let scale = ((mag - dz) / (1.0 - dz)).min(1.0).powf(curve);
    (x / mag * scale, y / mag * scale)
}

fn tap_action(action: &str, key: &str) {
    match action {
        "mouse:left" => click_mouse(0x0002, 0x0004),
        "mouse:right" => click_mouse(0x0008, 0x0010),
        "mouse:middle" => click_mouse(0x0020, 0x0040),
        "mouse:double" => {
            click_mouse(0x0002, 0x0004);
            click_mouse(0x0002, 0x0004);
        }
        "media:playpause" => tap_key(0xB3),
        "media:next" => tap_key(0xB0),
        "media:prev" => tap_key(0xB1),
        "media:volup" => tap_key(0xAF),
        "media:voldown" => tap_key(0xAE),
        "media:mute" => tap_key(0xAD),
        "key" => tap_combo(key),
        _ => {}
    }
}

fn click_mouse(down: u32, up: u32) {
    mouse_button(down);
    mouse_button(up);
}

fn tap_combo(combo: &str) {
    let Some(keys) = parse_combo(combo) else {
        return;
    };
    for key in &keys {
        key_event(*key, false);
    }
    for key in keys.iter().rev() {
        key_event(*key, true);
    }
}

fn tap_key(key: u16) {
    key_event(key, false);
    key_event(key, true);
}

fn parse_combo(combo: &str) -> Option<Vec<u16>> {
    let mut keys = Vec::new();
    for part in combo
        .to_ascii_lowercase()
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let key = match part {
            "ctrl" | "control" => 0x11,
            "shift" => 0x10,
            "alt" => 0x12,
            "win" => 0x5B,
            "enter" | "return" => 0x0D,
            "esc" | "escape" => 0x1B,
            "space" => 0x20,
            "tab" => 0x09,
            "backspace" => 0x08,
            "delete" | "del" => 0x2E,
            "insert" => 0x2D,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" => 0x21,
            "pagedown" => 0x22,
            "left" => 0x25,
            "up" => 0x26,
            "right" => 0x27,
            "down" => 0x28,
            "printscreen" => 0x2C,
            "capslock" => 0x14,
            "," => 0xBC,
            "." => 0xBE,
            "-" => 0xBD,
            "=" => 0xBB,
            "/" => 0xBF,
            ";" => 0xBA,
            _ if part.len() == 1 && part.as_bytes()[0].is_ascii_alphanumeric() => {
                part.as_bytes()[0].to_ascii_uppercase() as u16
            }
            _ if part.starts_with('f')
                && part[1..]
                    .parse::<u16>()
                    .is_ok_and(|n| (1..=24).contains(&n)) =>
            {
                0x6F + part[1..].parse::<u16>().ok()?
            }
            _ => return None,
        };
        keys.push(key);
    }
    if keys.is_empty() { None } else { Some(keys) }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct KeyboardInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct HardwareInput {
    message: u32,
    low: u16,
    high: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
union InputData {
    mouse: MouseInput,
    keyboard: KeyboardInput,
    hardware: HardwareInput,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    kind: u32,
    data: InputData,
}

#[cfg(windows)]
mod input_api {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
    }
    pub fn send(input: Input) {
        unsafe {
            SendInput(1, &input, std::mem::size_of::<Input>() as i32);
        }
    }
    pub fn cursor() -> Point {
        let mut point = Point { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point);
        }
        point
    }
    pub fn set_cursor(x: i32, y: i32) {
        unsafe {
            SetCursorPos(x, y);
        }
    }
}

fn mouse_button(flags: u32) {
    #[cfg(windows)]
    input_api::send(Input {
        kind: 0,
        data: InputData {
            mouse: MouseInput {
                dx: 0,
                dy: 0,
                mouse_data: 0,
                flags,
                time: 0,
                extra: 0,
            },
        },
    });
}
fn mouse_wheel(amount: i32) {
    #[cfg(windows)]
    input_api::send(Input {
        kind: 0,
        data: InputData {
            mouse: MouseInput {
                dx: 0,
                dy: 0,
                mouse_data: amount as u32,
                flags: 0x0800,
                time: 0,
                extra: 0,
            },
        },
    });
}
fn key_event(vk: u16, up: bool) {
    let extended = matches!(vk, 0x21..=0x2E | 0x5B);
    let flags = (if up { 0x0002 } else { 0 }) | if extended { 0x0001 } else { 0 };
    #[cfg(windows)]
    input_api::send(Input {
        kind: 1,
        data: InputData {
            keyboard: KeyboardInput {
                vk,
                scan: 0,
                flags,
                time: 0,
                extra: 0,
            },
        },
    });
    #[cfg(not(windows))]
    let _ = (vk, flags);
}
fn move_cursor_by(dx: i32, dy: i32) {
    #[cfg(windows)]
    {
        let point = input_api::cursor();
        input_api::set_cursor(point.x + dx, point.y + dy);
    }
    #[cfg(not(windows))]
    let _ = (dx, dy);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Dashboard,
    Mouse,
    Buttons,
    Settings,
}

struct ControllerApp {
    config: Config,
    persisted_config: Config,
    controller: Controller,
    page: Page,
    startup: bool,
    _tray: Option<TrayIcon>,
    menu_open: MenuItem,
    menu_close: MenuItem,
    menu_toggle: MenuItem,
    menu_quit: MenuItem,
    tray_error: Option<String>,
    minimized_on_launch: bool,
    has_hidden_once: bool,
    last_frame: Instant,
    started: Instant,
    toast: Option<(String, Instant)>,
    request_quit: bool,
    show_reset: bool,
    show_touch_error: Option<String>,
    app_icon: egui::TextureHandle,
}

impl ControllerApp {
    fn new(cc: &eframe::CreationContext<'_>, minimized: bool) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(15.0, 9.0);
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = FIELD;
        style.visuals.widgets.noninteractive.bg_fill = PANEL;
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
        style.visuals.widgets.inactive.bg_fill = PANEL_HI;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 75, 77);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
        style.visuals.widgets.active.bg_fill = MINT_DIM;
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, MINT);
        style.visuals.selection.bg_fill = MINT_DIM;
        style.visuals.selection.stroke = Stroke::new(1.0_f32, MINT);
        style.visuals.window_corner_radius = egui::CornerRadius::same(14);
        cc.egui_ctx.set_style(style);

        let mut config = Config::load();
        #[cfg(windows)]
        {
            TOUCH_KEYBOARD_ACTIVE.store(config.touch_keyboard, Ordering::Relaxed);
            start_global_text_focus_watcher();
        }
        if config.touch_keyboard {
            let _ = set_touch_keyboard(&mut config, true);
        }
        let icon_image =
            image::load_from_memory(include_bytes!("../assets/controller-control-icon-256.png"))
                .expect("the embedded application icon is valid")
                .into_rgba8();
        let app_icon = cc.egui_ctx.load_texture(
            "application-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon_image.width() as usize, icon_image.height() as usize],
                icon_image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let (menu_open, menu_close, menu_toggle, menu_quit, tray, tray_error) = create_tray();
        Self {
            startup: startup_enabled(),
            persisted_config: config.clone(),
            config,
            controller: Controller::new(),
            page: Page::Dashboard,
            _tray: tray,
            menu_open,
            menu_close,
            menu_toggle,
            menu_quit,
            tray_error,
            minimized_on_launch: minimized,
            has_hidden_once: false,
            last_frame: Instant::now(),
            started: Instant::now(),
            toast: None,
            request_quit: false,
            show_reset: false,
            show_touch_error: None,
            app_icon,
        }
    }

    fn set_toast(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }

    fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0_f32, EDGE))
            .corner_radius(egui::CornerRadius::same(15))
            .inner_margin(egui::Margin::same(18))
            .show(ui, add)
            .inner
    }

    fn row(ui: &mut egui::Ui, title: &str, desc: &str, right: impl FnOnce(&mut egui::Ui)) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(14.0).strong().color(INK));
                ui.label(RichText::new(desc).size(11.5).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), right);
        });
        ui.add_space(9.0);
    }

    fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
        let text = if *value {
            format!("●  {label}")
        } else {
            format!("○  {label}")
        };
        let response = ui.add(
            egui::Button::new(RichText::new(text).color(if *value { MINT } else { MUTED }))
                .fill(if *value { MINT_DIM } else { FIELD })
                .stroke(Stroke::new(1.0_f32, EDGE)),
        );
        if response.clicked() {
            *value = !*value;
        }
        response
    }

    fn page_name(&self) -> (&'static str, &'static str) {
        match self.page {
            Page::Dashboard => ("Dashboard", "A live view of your controller"),
            Page::Mouse => ("Mouse", "Shape pointer movement and scrolling"),
            Page::Buttons => ("Buttons", "Choose what each controller input does"),
            Page::Settings => ("Settings", "Set how the app works with Windows"),
        }
    }

    fn process_tray_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: tray_icon::MouseButton::Left,
                    ..
                }
            ) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *self.menu_open.id() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if event.id == *self.menu_close.id() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else if event.id == *self.menu_toggle.id() {
                self.config.enabled = !self.config.enabled;
                if !self.config.enabled {
                    self.controller.release_all();
                }
                self.config.save();
            } else if event.id == *self.menu_quit.id() {
                self.controller.release_all();
                self.request_quit = true;
            }
        }
    }

    fn show_navigation(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.image((self.app_icon.id(), Vec2::splat(42.0)));
            ui.vertical(|ui| {
                ui.label(RichText::new("PAD / PC").size(10.5).strong().color(MINT));
                ui.label(
                    RichText::new("Controller Control")
                        .size(18.0)
                        .strong()
                        .color(INK),
                );
                ui.label(
                    RichText::new("A gamepad for your desktop")
                        .size(10.5)
                        .color(MUTED),
                );
            });
        });
        ui.add_space(23.0);

        for (page, title, help) in [
            (Page::Dashboard, "Overview", "Connection and live input"),
            (Page::Mouse, "Pointer", "Movement and scrolling"),
            (Page::Buttons, "Button mapping", "Actions for each input"),
            (
                Page::Settings,
                "Windows settings",
                "Startup and touch keyboard",
            ),
        ] {
            let selected = self.page == page;
            let button =
                egui::Button::new(RichText::new(title).size(14.0).strong().color(if selected {
                    INK
                } else {
                    MUTED
                }))
                .fill(if selected { MINT_DIM } else { PANEL })
                .stroke(Stroke::new(1.0_f32, if selected { MINT } else { EDGE }));
            if ui.add_sized([ui.available_width(), 40.0], button).clicked() {
                self.page = page;
            }
            ui.label(RichText::new(help).size(10.5).color(MUTED));
            ui.add_space(7.0);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(9.0);
        let (state, dot) = if !self.controller.error.is_empty() {
            ("Input service unavailable", RED)
        } else if self.controller.connected && self.config.enabled {
            ("Controller active", MINT)
        } else if self.controller.connected {
            ("Controller paused", Color32::from_rgb(245, 190, 86))
        } else {
            ("Waiting for controller", MUTED)
        };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, dot);
            ui.label(RichText::new(state).size(12.0).color(INK));
        });
        if let Some(error) = &self.tray_error {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("System tray unavailable: {error}"))
                    .size(10.5)
                    .color(RED),
            );
        }
        if !self.controller.error.is_empty() {
            ui.add_space(5.0);
            ui.label(RichText::new(&self.controller.error).size(10.5).color(RED));
        } else if self.controller.connected {
            ui.label(RichText::new(&self.controller.name).size(10.5).color(MUTED));
        }
    }

    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        let state_title = if self.controller.connected {
            "Controller ready"
        } else {
            "Waiting for a controller"
        };
        let state_color = if self.controller.connected {
            MINT
        } else {
            MUTED
        };
        Self::card(ui, |ui| {
            ui.label(RichText::new("Connection").size(16.0).strong().color(INK));
            ui.add_space(8.0);
            ui.label(
                RichText::new(state_title)
                    .size(21.0)
                    .strong()
                    .color(state_color),
            );
            ui.label(
                RichText::new(if self.controller.connected {
                    &self.controller.name
                } else if self.controller.error.is_empty() {
                    "Connect a controller, then choose it below"
                } else {
                    "Controller input could not start. Check the message in the navigation panel."
                })
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(12.0);
            Self::row(
                ui,
                "Controller control",
                "Turn gamepad mouse and button actions on or off",
                |ui| {
                    let was_enabled = self.config.enabled;
                    let response = Self::toggle(
                        ui,
                        &mut self.config.enabled,
                        if was_enabled { "On" } else { "Off" },
                    );
                    if response.clicked() && was_enabled && !self.config.enabled {
                        self.controller.release_all();
                    }
                },
            );
            Self::row(
                ui,
                "Virtual Xbox 360 controller",
                "Let games and Windows see this pad as an Xbox controller (ViGEmBus required)",
                |ui| {
                    let enabled = self.config.xbox_output;
                    if Self::toggle(
                        ui,
                        &mut self.config.xbox_output,
                        if enabled { "On" } else { "Off" },
                    )
                    .clicked()
                    {
                        self.config.save();
                    }
                },
            );
            if !self.controller.xbox_error.is_empty() && self.config.xbox_output {
                ui.label(
                    RichText::new(&self.controller.xbox_error)
                        .size(11.5)
                        .color(RED),
                );
            }
            let options: Vec<String> = self
                .controller
                .devices
                .iter()
                .map(|(_, name)| name.clone())
                .collect();
            Self::row(
                ui,
                "Active controller",
                "Select which connected pad to use",
                |ui| {
                    let selected = options
                        .get(self.config.device)
                        .cloned()
                        .unwrap_or_else(|| "No controller found".into());
                    egui::ComboBox::from_id_salt("device-select")
                        .selected_text(selected)
                        .width(ui.available_width().min(280.0))
                        .show_ui(ui, |ui| {
                            if options.is_empty() {
                                ui.label("Connect a controller and reopen this list");
                            }
                            for (index, name) in options.iter().enumerate() {
                                ui.selectable_value(&mut self.config.device, index, name);
                            }
                        });
                },
            );
        });
        ui.add_space(14.0);
        Self::card(ui, |ui| {
            ui.label(RichText::new("Live input").size(16.0).strong().color(INK));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                draw_stick(
                    ui,
                    "Left stick",
                    self.controller.sticks[0],
                    self.controller.sticks[1],
                );
                draw_stick(
                    ui,
                    "Right stick",
                    self.controller.sticks[2],
                    self.controller.sticks[3],
                );
            });
            let pressed: Vec<String> = self
                .controller
                .pressed
                .iter()
                .map(|p| input_label(p))
                .collect();
            ui.label(
                RichText::new(if pressed.is_empty() {
                    "No buttons pressed".into()
                } else {
                    format!("Pressed: {}", pressed.join(", "))
                })
                .size(12.0)
                .color(if pressed.is_empty() { MUTED } else { INK }),
            );
        });
        ui.add_space(14.0);
        Self::card(ui, |ui| {
            ui.label(
                RichText::new("Quick adjustments")
                    .size(16.0)
                    .strong()
                    .color(INK),
            );
            ui.label(
                RichText::new("Changes save automatically")
                    .size(11.5)
                    .color(MUTED),
            );
            ui.add_space(10.0);
            slider(
                ui,
                "Cursor speed",
                &mut self.config.mouse_speed,
                100.0..=4000.0,
                50.0,
                |v| format!("{v:.0} px/s"),
            );
            slider(
                ui,
                "Scroll speed",
                &mut self.config.scroll_speed,
                1.0..=40.0,
                1.0,
                |v| format!("{v:.0} lines/s"),
            );
        });
    }

    fn show_mouse(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.label(
                RichText::new("Cursor movement")
                    .size(16.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(8.0);
            Self::row(
                ui,
                "Move pointer with",
                "Choose which stick controls the cursor",
                |ui| {
                    choice(
                        ui,
                        "mouse-stick",
                        &mut self.config.mouse_stick,
                        &["left", "right", "none"],
                        &["Left stick", "Right stick", "Off"],
                    );
                },
            );
            slider(
                ui,
                "Speed",
                &mut self.config.mouse_speed,
                100.0..=4000.0,
                50.0,
                |v| format!("{v:.0} px/s"),
            );
            slider(
                ui,
                "Response curve",
                &mut self.config.curve,
                1.0..=3.0,
                0.1,
                |v| format!("{v:.1}"),
            );
            slider(
                ui,
                "Dead zone",
                &mut self.config.deadzone,
                0.0..=0.5,
                0.01,
                |v| format!("{:.0}%", v * 100.0),
            );
            slider(
                ui,
                "Slow cursor factor",
                &mut self.config.precision_factor,
                0.05..=1.0,
                0.05,
                |v| format!("×{v:.2}"),
            );
            slider(
                ui,
                "Fast cursor factor",
                &mut self.config.boost_factor,
                1.0..=4.0,
                0.1,
                |v| format!("×{v:.1}"),
            );
            Self::row(
                ui,
                "Reverse vertical movement",
                "Push up to move the pointer down",
                |ui| {
                    let label = if self.config.invert_mouse_y {
                        "Reversed"
                    } else {
                        "Normal"
                    };
                    Self::toggle(ui, &mut self.config.invert_mouse_y, label);
                },
            );
        });
        ui.add_space(12.0);
        Self::card(ui, |ui| {
            ui.label(
                RichText::new("Scrolling and repeat")
                    .size(16.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(8.0);
            Self::row(
                ui,
                "Scroll with",
                "Choose a stick or turn stick scrolling off",
                |ui| {
                    choice(
                        ui,
                        "scroll-stick",
                        &mut self.config.scroll_stick,
                        &["left", "right", "none"],
                        &["Left stick", "Right stick", "Off"],
                    );
                },
            );
            slider(
                ui,
                "Scroll speed",
                &mut self.config.scroll_speed,
                1.0..=40.0,
                1.0,
                |v| format!("{v:.0} lines/s"),
            );
            Self::row(
                ui,
                "Reverse scrolling",
                "Use natural scrolling direction",
                |ui| {
                    let label = if self.config.invert_scroll {
                        "Reversed"
                    } else {
                        "Normal"
                    };
                    Self::toggle(ui, &mut self.config.invert_scroll, label);
                },
            );
            slider(
                ui,
                "Repeat delay",
                &mut self.config.repeat_delay,
                0.1..=1.0,
                0.05,
                |v| format!("{v:.2} s"),
            );
            slider(
                ui,
                "Repeat rate",
                &mut self.config.repeat_rate,
                0.02..=0.3,
                0.01,
                |v| format!("{v:.2} s"),
            );
        });
    }

    fn show_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("Add button action").color(BG).strong())
                        .fill(MINT),
                )
                .clicked()
            {
                self.config.bindings.push(Binding {
                    input: "none".into(),
                    action: "none".into(),
                    key: String::new(),
                });
            }
            if ui.button("Restore defaults").clicked() {
                self.show_reset = true;
            }
        });
        ui.label(
            RichText::new("Choose Learn, then press a button or D-pad direction.")
                .size(11.5)
                .color(MUTED),
        );
        ui.add_space(8.0);
        {
            let mut delete = None;
            let mut learn = None;
            for index in 0..self.config.bindings.len() {
                let binding = &mut self.config.bindings[index];
                let is_learning = self.controller.learning == Some(index);
                Self::card(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(
                                RichText::new(if is_learning {
                                    "Press a controller button…".to_owned()
                                } else {
                                    format!("Learn: {}", input_label(&binding.input))
                                })
                                .color(if is_learning {
                                    MINT
                                } else {
                                    INK
                                }),
                            )
                            .clicked()
                        {
                            learn = Some(index);
                        }
                        ui.label(RichText::new("Action").color(MUTED));
                        egui::ComboBox::from_id_salt(("action", index))
                            .selected_text(action_label(&binding.action))
                            .width(260.0)
                            .show_ui(ui, |ui| {
                                for (id, label) in ACTIONS {
                                    ui.selectable_value(
                                        &mut binding.action,
                                        (*id).to_owned(),
                                        *label,
                                    );
                                }
                            });
                        if ui.button(RichText::new("Remove").color(RED)).clicked() {
                            delete = Some(index);
                        }
                    });
                    if binding.action == "key" {
                        ui.add(
                            egui::TextEdit::singleline(&mut binding.key)
                                .hint_text("For example: ctrl+shift+esc")
                                .desired_width(ui.available_width().min(360.0)),
                        );
                    }
                });
                ui.add_space(4.0);
            }
            if let Some(index) = delete {
                self.config.bindings.remove(index);
                self.controller.binding_states.clear();
                self.controller.learning = None;
            }
            if let Some(index) = learn {
                self.controller.learning = Some(index);
                self.toast = Some((
                    "Press any controller button or D-pad direction".into(),
                    Instant::now(),
                ));
            }
        }
        ui.add_space(7.0);
        ui.label(
            RichText::new("Examples: ctrl+c, alt+tab, win+d, f11, enter, esc")
                .size(11.0)
                .color(MUTED),
        );
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.label(
                RichText::new("Windows integration")
                    .size(16.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(9.0);
            Self::row(
                ui,
                "Start with Windows",
                "Open quietly in the tray after you sign in",
                |ui| {
                    let mut enabled = self.startup;
                    let label = if enabled { "On" } else { "Off" };
                    if Self::toggle(ui, &mut enabled, label).clicked() {
                        match set_startup(enabled) {
                            Ok(()) => {
                                self.startup = enabled;
                                self.set_toast(if enabled {
                                    "Startup enabled"
                                } else {
                                    "Startup disabled"
                                });
                            }
                            Err(e) => self.set_toast(format!("Could not update startup: {e}")),
                        }
                    }
                },
            );
            Self::row(
                ui,
                "Keep running when closed",
                "Close to the system tray instead of quitting",
                |ui| {
                    let label = if self.config.keep_running {
                        "On"
                    } else {
                        "Off"
                    };
                    Self::toggle(ui, &mut self.config.keep_running, label);
                },
            );
            Self::row(
                ui,
                "Show touch keyboard in Windows",
                "Show Windows touch keyboard when an editable field gets focus in any app",
                |ui| {
                    let mut enabled = self.config.touch_keyboard;
                    let label = if enabled { "On" } else { "Off" };
                    if Self::toggle(ui, &mut enabled, label).clicked() {
                        if let Err(e) = set_touch_keyboard(&mut self.config, enabled) {
                            self.config.touch_keyboard = !enabled;
                            self.show_touch_error = Some(format!(
                                "Could not update the Windows touch keyboard setting.\n{e}"
                            ));
                        } else {
                            self.set_toast(if enabled {
                                "Windows touch keyboard enabled system-wide"
                            } else {
                                "Windows touch keyboard restored to its previous setting"
                            });
                        }
                    }
                },
            );
            ui.add_space(8.0);
            ui.label(RichText::new("The Windows setting is saved for your account. Turning this off restores the value that was in place before you enabled it here.").size(11.5).color(MUTED));
        });
        ui.add_space(12.0);
        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Reset settings")
                            .size(15.0)
                            .strong()
                            .color(INK),
                    );
                    ui.label(
                        RichText::new("Restore pointer, scroll and controller bindings")
                            .size(11.5)
                            .color(MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new("Reset to defaults").color(RED))
                        .clicked()
                    {
                        self.show_reset = true;
                    }
                });
            });
        });
        ui.add_space(12.0);
        Self::card(ui, |ui| {
            ui.label(RichText::new("About").size(15.0).strong().color(INK));
            ui.label(
                RichText::new(format!(
                    "Settings are stored in {}",
                    Config::path().display()
                ))
                .size(11.5)
                .color(MUTED),
            );
            ui.label(
                RichText::new("Button labels use an Xbox layout. Use Learn for other controllers.")
                    .size(11.5)
                    .color(MUTED),
            );
        });
    }
}

fn slider(
    ui: &mut egui::Ui,
    title: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    format_value: impl Fn(f32) -> String,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(12.0).color(INK));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format_value(*value))
                    .size(12.0)
                    .strong()
                    .color(MINT),
            );
        });
    });
    ui.add(
        egui::Slider::new(value, range)
            .step_by(step)
            .show_value(false)
            .trailing_fill(true),
    );
    ui.add_space(7.0);
}

fn choice(ui: &mut egui::Ui, id: &str, current: &mut String, values: &[&str], labels: &[&str]) {
    let selected = values.iter().position(|v| *v == current).unwrap_or(0);
    egui::ComboBox::from_id_salt(id)
        .selected_text(labels.get(selected).copied().unwrap_or("Off"))
        .show_ui(ui, |ui| {
            for (value, label) in values.iter().zip(labels) {
                ui.selectable_value(current, (*value).to_owned(), *label);
            }
        });
}

fn draw_stick(ui: &mut egui::Ui, label: &str, x: f32, y: f32) {
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(118.0, 112.0), Sense::hover());
        let center = rect.center() - Vec2::new(0.0, 5.0);
        let radius = 39.0;
        ui.painter().circle_filled(center, radius, FIELD);
        ui.painter()
            .circle_stroke(center, radius, Stroke::new(1.5_f32, EDGE));
        ui.painter().line_segment(
            [
                center - Vec2::new(radius, 0.0),
                center + Vec2::new(radius, 0.0),
            ],
            Stroke::new(1.0_f32, EDGE),
        );
        ui.painter().line_segment(
            [
                center - Vec2::new(0.0, radius),
                center + Vec2::new(0.0, radius),
            ],
            Stroke::new(1.0_f32, EDGE),
        );
        ui.painter().circle_filled(
            center + Vec2::new(x * radius * 0.75, y * radius * 0.75),
            7.0,
            MINT,
        );
        ui.painter().text(
            rect.center_bottom() - Vec2::new(0.0, 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            FontId::proportional(10.0),
            MUTED,
        );
    });
}

fn create_tray() -> (
    MenuItem,
    MenuItem,
    MenuItem,
    MenuItem,
    Option<TrayIcon>,
    Option<String>,
) {
    let open = MenuItem::new("Open Controller Control", true, None);
    let close = MenuItem::new("Close window", true, None);
    let toggle = MenuItem::new("Toggle controller control", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    let result = (|| -> Result<TrayIcon, String> {
        menu.append(&open)
            .map_err(|error| format!("Could not add Open menu item: {error}"))?;
        menu.append(&close)
            .map_err(|error| format!("Could not add Close menu item: {error}"))?;
        menu.append(&toggle)
            .map_err(|error| format!("Could not add Toggle menu item: {error}"))?;
        menu.append(&quit)
            .map_err(|error| format!("Could not add Quit menu item: {error}"))?;

        let image =
            image::load_from_memory(include_bytes!("../assets/controller-control-icon-256.png"))
                .map_err(|error| format!("Could not load tray icon: {error}"))?;
        let rgba = image::imageops::resize(
            &image.to_rgba8(),
            64,
            64,
            image::imageops::FilterType::Lanczos3,
        )
        .into_raw();
        let icon = Icon::from_rgba(rgba, 64, 64)
            .map_err(|error| format!("Could not create tray icon: {error}"))?;
        TrayIconBuilder::new()
            .with_tooltip(APP_TITLE)
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .build()
            .map_err(|error| format!("Could not create system tray entry: {error}"))
    })();
    match result {
        Ok(tray) => (open, close, toggle, quit, Some(tray), None),
        Err(error) => (open, close, toggle, quit, None, Some(error)),
    }
}

impl eframe::App for ControllerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;
        self.controller
            .poll(&mut self.config, dt, self.started.elapsed().as_secs_f64());
        self.process_tray_events(ctx);

        if self.minimized_on_launch && !self.has_hidden_once {
            // Never start invisible when tray initialization failed; there would
            // be no way for the user to bring the window back.
            if self._tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            self.has_hidden_once = true;
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            if self.config.keep_running && self._tray.is_some() && !self.request_quit {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                self.controller.release_all();
            }
        }
        if self.request_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if self.config != self.persisted_config {
            self.config.save();
            self.persisted_config = self.config.clone();
        }

        egui::SidePanel::left("main_navigation")
            .resizable(false)
            .exact_width(228.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::same(17))
                    .stroke(Stroke::new(1.0_f32, EDGE)),
            )
            .show(ctx, |ui| self.show_navigation(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(22)),
            )
            .show(ctx, |ui| {
                let (title, subtitle) = self.page_name();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).size(27.0).strong().color(INK));
                        ui.label(RichText::new(subtitle).size(12.5).color(MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (text, color) = if self.controller.connected {
                            ("Connected", MINT)
                        } else {
                            ("Disconnected", MUTED)
                        };
                        ui.label(
                            RichText::new(format!("●  {text}"))
                                .size(11.0)
                                .strong()
                                .color(color),
                        );
                    });
                });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(12.0);

                egui::ScrollArea::vertical()
                    .id_salt("active_page_content")
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Dashboard => self.show_dashboard(ui),
                        Page::Mouse => self.show_mouse(ui),
                        Page::Buttons => self.show_buttons(ui),
                        Page::Settings => self.show_settings(ui),
                    });
            });

        if let Some((message, time)) = &self.toast {
            if time.elapsed().as_secs_f32() < 4.0 {
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-22.0, -20.0))
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(PANEL_HI)
                            .stroke(Stroke::new(1.0_f32, EDGE))
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(message).size(12.0).color(INK));
                            });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            } else {
                self.toast = None;
            }
        }

        if self.show_reset {
            egui::Window::new("Restore defaults")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Reset pointer settings and all controller bindings?");
                    ui.horizontal(|ui| {
                        if ui.button("Reset").clicked() {
                            if self.config.touch_keyboard {
                                let _ = set_touch_keyboard(&mut self.config, false);
                            }
                            self.config = Config::default();
                            self.controller.release_all();
                            self.controller.binding_states.clear();
                            self.controller.learning = None;
                            self.show_reset = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_reset = false;
                        }
                    });
                });
        }
        if let Some(error) = self.show_touch_error.clone() {
            egui::Window::new("Touch keyboard setting")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(error);
                    if ui.button("Close").clicked() {
                        self.show_touch_error = None;
                    }
                });
        }

        // The controller is sampled every frame; a moderate refresh keeps cursor response snappy.
        ctx.request_repaint_after(std::time::Duration::from_millis(10));
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .map(|v| v.to_ascii_lowercase())
        .collect();
    if args.iter().any(|v| v == "--startup-on") {
        let _ = set_startup(true);
        return Ok(());
    }
    if args.iter().any(|v| v == "--startup-off") {
        let _ = set_startup(false);
        return Ok(());
    }
    let minimized = args.iter().any(|v| v == "--minimized");
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(
            attributes: *const std::ffi::c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> isize;
        fn GetLastError() -> u32;
    }
    let mutex_name: Vec<u16> = "Global\\ControllerToPC_single"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let _mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if unsafe { GetLastError() } == 183 {
        return Ok(());
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_TITLE)
        .with_inner_size([1040.0, 700.0])
        .with_min_inner_size([850.0, 580.0])
        .with_icon(egui::IconData {
            rgba: image::load_from_memory(include_bytes!(
                "../assets/controller-control-icon-256.png"
            ))
            .expect("the embedded application icon is valid")
            .into_rgba8()
            .into_raw(),
            width: 256,
            height: 256,
        });
    if minimized {
        viewport = viewport.with_visible(false);
    }
    eframe::run_native(
        APP_TITLE,
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(ControllerApp::new(cc, minimized)))),
    )
}
