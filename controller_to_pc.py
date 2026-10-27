"""
Controller Control - use a game controller as a mouse + media remote (Windows).

Command line:
  controller_to_pc.exe                 open the app
  controller_to_pc.exe --minimized     start hidden in the tray (used for startup)
  controller_to_pc.exe --startup-on    start with Windows
  controller_to_pc.exe --startup-off   stop starting with Windows
"""
import copy
import ctypes
import json
import math
import os
import subprocess
import sys
import threading
import time
import tkinter as tk
from tkinter import font as tkfont
from tkinter import messagebox

APP_NAME = "ControllerToPC"
APP_TITLE = "Controller Control"
IS_WIN = os.name == "nt"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
CONFIG_DIR = os.path.join(os.environ.get("APPDATA", os.path.expanduser("~")), APP_NAME)
CONFIG_PATH = os.path.join(CONFIG_DIR, "config.json")

# ----------------------------------------------------------------------------
# Palette
# ----------------------------------------------------------------------------
BG = "#10191d"          # graphite base, inspired by a controller's hardware shell
CARD = "#19272b"
CARD_HI = "#23363a"
BORDER = "#30464a"
TEXT = "#eff5ef"
MUTED = "#9aadaa"
ACCENT = "#65d8bd"
ACCENT_HI = "#8be8d3"
ACCENT_DIM = "#244b45"
BTN = "#263a3d"
BTN_HI = "#345052"
GOOD = "#65d8bd"
BAD = "#ff8073"
FIELD = "#111d20"
FONT = "Segoe UI"

SCALE = 1.0


def S(v):
    return int(round(v * SCALE))


def F(size, bold=False):
    return (FONT, -S(size), "bold" if bold else "normal")


# ----------------------------------------------------------------------------
# Operating-system layer (mouse / keyboard output)
# ----------------------------------------------------------------------------
OSLOG = []   # only used off-Windows (for tests)

if IS_WIN:
    from ctypes import wintypes
    _u = ctypes.windll.user32
    _u.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
    _u.mouse_event.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.DWORD,
                               wintypes.DWORD, ctypes.c_size_t]
    _u.keybd_event.argtypes = [wintypes.BYTE, wintypes.BYTE, wintypes.DWORD, ctypes.c_size_t]

    class _POINT(ctypes.Structure):
        _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]

    def cursor_pos():
        p = _POINT()
        _u.GetCursorPos(ctypes.byref(p))
        return p.x, p.y

    def set_cursor(x, y):
        _u.SetCursorPos(int(x), int(y))

    def mouse_event(flags, data=0):
        _u.mouse_event(flags, 0, 0, data & 0xFFFFFFFF, 0)

    def key_event(vk, up=False, extended=False):
        _u.keybd_event(vk, 0, (2 if up else 0) | (1 if extended else 0), 0)
else:
    _pos = [500, 500]

    def cursor_pos():
        return tuple(_pos)

    def set_cursor(x, y):
        _pos[0], _pos[1] = int(x), int(y)
        OSLOG.append(("move", int(x), int(y)))

    def mouse_event(flags, data=0):
        OSLOG.append(("mouse", flags, data))

    def key_event(vk, up=False, extended=False):
        OSLOG.append(("key", vk, "up" if up else "down"))


MOUSE_LDOWN, MOUSE_LUP = 0x0002, 0x0004
MOUSE_RDOWN, MOUSE_RUP = 0x0008, 0x0010
MOUSE_MDOWN, MOUSE_MUP = 0x0020, 0x0040
MOUSE_WHEEL = 0x0800

VK = {
    "ctrl": 0x11, "control": 0x11, "shift": 0x10, "alt": 0x12, "win": 0x5B,
    "enter": 0x0D, "return": 0x0D, "esc": 0x1B, "escape": 0x1B, "space": 0x20,
    "tab": 0x09, "backspace": 0x08, "delete": 0x2E, "del": 0x2E, "insert": 0x2D,
    "home": 0x24, "end": 0x23, "pageup": 0x21, "pagedown": 0x22,
    "left": 0x25, "up": 0x26, "right": 0x27, "down": 0x28,
    "printscreen": 0x2C, "capslock": 0x14,
    "media_play_pause": 0xB3, "media_next": 0xB0, "media_prev": 0xB1,
    "volume_up": 0xAF, "volume_down": 0xAE, "volume_mute": 0xAD,
    ",": 0xBC, ".": 0xBE, "-": 0xBD, "=": 0xBB, "/": 0xBF, ";": 0xBA,
}
EXTENDED = {0x25, 0x26, 0x27, 0x28, 0x24, 0x23, 0x21, 0x22, 0x2D, 0x2E, 0x5B}
_combo_cache = {}


def parse_combo(text):
    """'ctrl+shift+esc' -> [0x11, 0x10, 0x1B]; returns [] when invalid."""
    text = (text or "").strip().lower()
    if text in _combo_cache:
        return _combo_cache[text]
    out = []
    for part in [p.strip() for p in text.split("+") if p.strip()]:
        if part in VK:
            out.append(VK[part])
        elif len(part) == 1 and part.isalnum():
            out.append(ord(part.upper()))
        elif part.startswith("f") and part[1:].isdigit() and 1 <= int(part[1:]) <= 24:
            out.append(0x6F + int(part[1:]))
        else:
            out = []
            break
    _combo_cache[text] = out
    return out


def tap_keys(vks):
    for vk in vks:
        key_event(vk, False, vk in EXTENDED)
    for vk in reversed(vks):
        key_event(vk, True, vk in EXTENDED)


# ----------------------------------------------------------------------------
# Actions + default configuration
# ----------------------------------------------------------------------------
ACTIONS = [
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
    ("key", "Keyboard shortcut..."),
]
ACTION_LABEL = dict(ACTIONS)
MEDIA_KEYS = {
    "media:playpause": "media_play_pause", "media:next": "media_next",
    "media:prev": "media_prev", "media:volup": "volume_up",
    "media:voldown": "volume_down", "media:mute": "volume_mute",
}
REPEATING = {"media:volup", "media:voldown", "key"}

BUTTON_NAMES = {0: "A", 1: "B", 2: "X", 3: "Y", 4: "LB", 5: "RB",
                6: "Back", 7: "Start", 8: "L3 (stick click)", 9: "R3 (stick click)"}


def input_name(inp):
    if not inp or inp == "none":
        return "Not set"
    kind, _, val = inp.partition(":")
    if kind == "btn":
        n = int(val)
        return BUTTON_NAMES.get(n, "Button %d" % n)
    if kind == "hat":
        return "D-pad " + val.capitalize()
    if kind == "axis+":
        n = int(val)
        return {4: "LT (left trigger)", 5: "RT (right trigger)"}.get(n, "Axis %d" % n)
    return inp


DEFAULTS = {
    "version": 1,
    "enabled": True,
    "device": 0,
    "mouse_stick": "left",
    "scroll_stick": "right",
    "mouse_speed": 1200,
    "curve": 1.5,
    "deadzone": 0.15,
    "invert_mouse_y": False,
    "scroll_speed": 12,
    "invert_scroll": False,
    "precision_factor": 0.3,
    "boost_factor": 2.0,
    "repeat_delay": 0.4,
    "repeat_rate": 0.08,
    "glass": True,
    "keep_running": True,
    "touch_keyboard": False,
    "bindings": [
        {"input": "btn:0", "action": "mouse:left_hold", "key": ""},
        {"input": "btn:1", "action": "mouse:right", "key": ""},
        {"input": "btn:2", "action": "media:playpause", "key": ""},
        {"input": "btn:4", "action": "media:prev", "key": ""},
        {"input": "btn:5", "action": "media:next", "key": ""},
        {"input": "hat:up", "action": "media:volup", "key": ""},
        {"input": "hat:down", "action": "media:voldown", "key": ""},
        {"input": "hat:left", "action": "key", "key": "left"},
        {"input": "hat:right", "action": "key", "key": "right"},
        {"input": "axis+:4", "action": "precision", "key": ""},
        {"input": "axis+:5", "action": "boost", "key": ""},
        {"input": "btn:7", "action": "app:toggle", "key": ""},
    ],
}


class Config:
    def __init__(self, path=CONFIG_PATH):
        self.path = path
        self.data = copy.deepcopy(DEFAULTS)
        self._timer = None
        self._root = None

    def load(self):
        try:
            with open(self.path, "r", encoding="utf-8") as f:
                saved = json.load(f)
            for k, v in saved.items():
                if k in DEFAULTS:
                    self.data[k] = v
            good = []
            for b in self.data.get("bindings", []):
                if isinstance(b, dict):
                    good.append({"input": str(b.get("input", "none")),
                                 "action": b.get("action", "none") if b.get("action") in ACTION_LABEL else "none",
                                 "key": str(b.get("key", ""))})
            self.data["bindings"] = good
        except (OSError, ValueError):
            pass
        return self

    def __getitem__(self, k):
        return self.data[k]

    def set(self, key, value):
        self.data[key] = value
        self.save_soon()

    def reset(self):
        self.data = copy.deepcopy(DEFAULTS)
        self.save()

    def save_soon(self):
        if self._root is None:
            return self.save()
        if self._timer:
            self._root.after_cancel(self._timer)
        self._timer = self._root.after(300, self.save)

    def save(self):
        self._timer = None
        try:
            os.makedirs(os.path.dirname(self.path), exist_ok=True)
            tmp = self.path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump(self.data, f, indent=2)
            os.replace(tmp, self.path)
        except OSError:
            pass


# ----------------------------------------------------------------------------
# Engine: reads the controller and drives mouse / keyboard
# ----------------------------------------------------------------------------
class Engine(threading.Thread):
    def __init__(self, cfg):
        super().__init__(daemon=True)
        self.cfg = cfg
        self.enabled = bool(cfg["enabled"])
        self.alive = True
        self.learn_cb = None
        self.state = {"connected": False, "name": "", "devices": [],
                      "sticks": (0.0, 0.0, 0.0, 0.0), "pressed": set(), "error": ""}
        self.prev = set()
        self.bstate = {}
        self.rem = [0.0, 0.0]
        self.scroll_acc = 0.0
        self.held_mouse = set()

    # -- control from the UI -------------------------------------------------
    def start_learn(self, cb):
        self.learn_cb = cb

    def cancel_learn(self):
        self.learn_cb = None

    def set_enabled(self, on):
        self.enabled = bool(on)
        self.cfg.data["enabled"] = self.enabled
        self.cfg.save_soon()
        if not self.enabled:
            self.release_all()

    def release_all(self):
        for b in list(self.held_mouse):
            self.mouse_button(b, False)
        for st in self.bstate.values():
            st["down"] = False

    # -- helpers --------------------------------------------------------------
    def mouse_button(self, which, down):
        flags = {"left": (MOUSE_LDOWN, MOUSE_LUP), "right": (MOUSE_RDOWN, MOUSE_RUP),
                 "middle": (MOUSE_MDOWN, MOUSE_MUP)}[which]
        mouse_event(flags[0] if down else flags[1])
        if down:
            self.held_mouse.add(which)
        else:
            self.held_mouse.discard(which)

    def tap(self, act, key):
        if act in MEDIA_KEYS:
            tap_keys(parse_combo(MEDIA_KEYS[act]))
        elif act == "key":
            tap_keys(parse_combo(key))
        elif act == "mouse:left":
            self.mouse_button("left", True); self.mouse_button("left", False)
        elif act == "mouse:right":
            self.mouse_button("right", True); self.mouse_button("right", False)
        elif act == "mouse:middle":
            self.mouse_button("middle", True); self.mouse_button("middle", False)
        elif act == "mouse:double":
            for _ in range(2):
                self.mouse_button("left", True); self.mouse_button("left", False)

    def shape(self, x, y):
        dz = min(max(float(self.cfg["deadzone"]), 0.0), 0.9)
        mag = math.hypot(x, y)
        if mag < dz or mag == 0:
            return 0.0, 0.0
        s = min(1.0, (mag - dz) / (1.0 - dz)) ** float(self.cfg["curve"])
        return x / mag * s, y / mag * s

    @staticmethod
    def pressed_inputs(snap):
        p = {"btn:%d" % i for i in snap["buttons"]}
        hx, hy = snap["hat"]
        if hy > 0: p.add("hat:up")
        if hy < 0: p.add("hat:down")
        if hx < 0: p.add("hat:left")
        if hx > 0: p.add("hat:right")
        for i, v in enumerate(snap["axes"]):
            if i >= 4 and v > 0.5:
                p.add("axis+:%d" % i)
        return p

    @staticmethod
    def stick(snap, which):
        a = snap["axes"]
        base = {"left": 0, "right": 2}.get(which)
        if base is None or len(a) < base + 2:
            return 0.0, 0.0
        return a[base], a[base + 1]

    # -- one tick --------------------------------------------------------------
    def process(self, snap, dt, now):
        cfg = self.cfg
        pressed = self.pressed_inputs(snap)
        new = pressed - self.prev
        self.prev = pressed

        lx, ly = self.stick(snap, "left")
        rx, ry = self.stick(snap, "right")
        self.state["sticks"] = (lx, ly, rx, ry)
        self.state["pressed"] = set(pressed)

        if self.learn_cb and new:
            cb, self.learn_cb = self.learn_cb, None
            cb(sorted(new)[0])
            return

        bindings = list(cfg["bindings"])
        live = {id(b) for b in bindings}
        for k in [k for k in self.bstate if k not in live]:
            del self.bstate[k]

        precision = boost = False
        scroll_dir = 0
        for b in bindings:
            act = b.get("action", "none")
            if act == "none" or b.get("input") in (None, "none"):
                continue
            is_down = b["input"] in pressed
            st = self.bstate.setdefault(id(b), {"down": False, "t": 0.0, "last": 0.0})
            if not self.enabled and act != "app:toggle":
                st["down"] = is_down
                continue
            if is_down and not st["down"]:
                st["down"] = True
                st["t"] = st["last"] = now
                if act == "mouse:left_hold":
                    self.mouse_button("left", True)
                elif act == "app:toggle":
                    self.set_enabled(not self.enabled)
                elif act not in ("precision", "boost", "scroll:up", "scroll:down"):
                    self.tap(act, b.get("key", ""))
            elif is_down and st["down"]:
                if (act in REPEATING and now - st["t"] > cfg["repeat_delay"]
                        and now - st["last"] > cfg["repeat_rate"]):
                    st["last"] = now
                    self.tap(act, b.get("key", ""))
            elif not is_down and st["down"]:
                st["down"] = False
                if act == "mouse:left_hold":
                    self.mouse_button("left", False)
            if st["down"]:
                if act == "precision": precision = True
                elif act == "boost": boost = True
                elif act == "scroll:up": scroll_dir += 1
                elif act == "scroll:down": scroll_dir -= 1

        if not self.enabled:
            return

        # cursor
        ms = cfg["mouse_stick"]
        if ms in ("left", "right"):
            sx, sy = self.shape(*self.stick(snap, ms))
            if cfg["invert_mouse_y"]:
                sy = -sy
            mult = (cfg["precision_factor"] if precision else 1.0) * (cfg["boost_factor"] if boost else 1.0)
            speed = float(cfg["mouse_speed"]) * mult
            self.rem[0] += sx * speed * dt
            self.rem[1] += sy * speed * dt
            ix, iy = int(self.rem[0]), int(self.rem[1])
            if ix or iy:
                self.rem[0] -= ix
                self.rem[1] -= iy
                cx, cy = cursor_pos()
                set_cursor(cx + ix, cy + iy)
        # scroll
        ss = cfg["scroll_stick"]
        notches = scroll_dir * float(cfg["scroll_speed"]) * dt
        if ss in ("left", "right"):
            _, sy = self.shape(*self.stick(snap, ss))
            notches += -sy * float(cfg["scroll_speed"]) * dt
        if cfg["invert_scroll"]:
            notches = -notches
        self.scroll_acc += notches
        n = int(self.scroll_acc)
        if n:
            self.scroll_acc -= n
            mouse_event(MOUSE_WHEEL, 120 * n)

    # -- thread loop ------------------------------------------------------------
    def run(self):
        os.environ.setdefault("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1")
        os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
        try:
            import pygame
        except Exception as exc:
            self.state["error"] = "pygame missing: %s" % exc
            return
        pygame.init()
        pygame.joystick.init()
        if IS_WIN:
            try:
                ctypes.windll.winmm.timeBeginPeriod(1)
            except Exception:
                pass
        pad, pad_index, last_scan = None, -1, 0.0
        last = time.perf_counter()
        while self.alive:
            t0 = time.perf_counter()
            dt = min(t0 - last, 0.1)
            last = t0
            try:
                pygame.event.pump()
                for ev in pygame.event.get():
                    if ev.type == pygame.JOYDEVICEREMOVED:
                        pad = None
                want = int(self.cfg["device"])
                if pad is not None and want != pad_index:
                    pad = None
                if pad is None and t0 - last_scan > 1.0:
                    last_scan = t0
                    if pygame.joystick.get_count() == 0:
                        pygame.joystick.quit()
                        pygame.joystick.init()
                    count = pygame.joystick.get_count()
                    names = []
                    for i in range(count):
                        try:
                            names.append(pygame.joystick.Joystick(i).get_name())
                        except pygame.error:
                            names.append("Controller %d" % (i + 1))
                    self.state["devices"] = names
                    if count:
                        pad_index = want if 0 <= want < count else 0
                        pad = pygame.joystick.Joystick(pad_index)
                        pad.init()
                        self.state["name"] = pad.get_name()
                if pad is None:
                    if self.state["connected"]:
                        self.release_all()
                    self.state["connected"] = False
                    self.state["sticks"] = (0.0, 0.0, 0.0, 0.0)
                    self.state["pressed"] = set()
                    time.sleep(0.05)
                    continue
                self.state["connected"] = True
                snap = {
                    "buttons": {i for i in range(pad.get_numbuttons()) if pad.get_button(i)},
                    "axes": [pad.get_axis(i) for i in range(pad.get_numaxes())],
                    "hat": pad.get_hat(0) if pad.get_numhats() else (0, 0),
                }
                self.process(snap, dt, t0)
            except pygame.error:
                pad = None
            hz = 125
            time.sleep(max(0.0, 1.0 / hz - (time.perf_counter() - t0)))
        self.release_all()


# ----------------------------------------------------------------------------
# Windows glass effect, startup entry
# ----------------------------------------------------------------------------
def apply_glass(root, on):
    """Acrylic blur behind the window. Returns True if it was applied."""
    if not IS_WIN:
        return False
    try:
        root.update_idletasks()
        hwnd = ctypes.windll.user32.GetParent(root.winfo_id()) or root.winfo_id()
        dwm = ctypes.windll.dwmapi
        dark = ctypes.c_int(1)
        dwm.DwmSetWindowAttribute(hwnd, 20, ctypes.byref(dark), 4)   # dark title bar
        build = sys.getwindowsversion().build
        if on:
            if build >= 22621:
                margins = (ctypes.c_int * 4)(-1, -1, -1, -1)
                dwm.DwmExtendFrameIntoClientArea(hwnd, ctypes.byref(margins))
                kind = ctypes.c_int(3)                                 # acrylic
                dwm.DwmSetWindowAttribute(hwnd, 38, ctypes.byref(kind), 4)
            else:
                class ACCENT(ctypes.Structure):
                    _fields_ = [("state", ctypes.c_int), ("flags", ctypes.c_int),
                                ("color", ctypes.c_uint), ("anim", ctypes.c_int)]

                class DATA(ctypes.Structure):
                    _fields_ = [("attr", ctypes.c_int), ("data", ctypes.c_void_p),
                                ("size", ctypes.c_size_t)]
                acc = ACCENT(4, 2, 0xB0140C0A, 0)                      # AABBGGRR tint
                data = DATA(19, ctypes.cast(ctypes.byref(acc), ctypes.c_void_p), ctypes.sizeof(acc))
                ctypes.windll.user32.SetWindowCompositionAttribute(hwnd, ctypes.byref(data))
            root.attributes("-transparentcolor", BG)
        else:
            root.attributes("-transparentcolor", "")
            if build >= 22621:
                kind = ctypes.c_int(1)                                 # none
                dwm.DwmSetWindowAttribute(hwnd, 38, ctypes.byref(kind), 4)
                margins = (ctypes.c_int * 4)(0, 0, 0, 0)
                dwm.DwmExtendFrameIntoClientArea(hwnd, ctypes.byref(margins))
        return True
    except Exception:
        return False


def launch_command():
    if getattr(sys, "frozen", False):
        return '"%s" --minimized' % sys.executable
    return '"%s" "%s" --minimized' % (sys.executable, os.path.abspath(__file__))


def startup_enabled():
    if not IS_WIN:
        return False
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as key:
            winreg.QueryValueEx(key, APP_NAME)
            return True
    except OSError:
        return False


def set_startup(enable):
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        if enable:
            winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, launch_command())
        else:
            try:
                winreg.DeleteValue(key, APP_NAME)
            except FileNotFoundError:
                pass


def popup(text):
    try:
        ctypes.windll.user32.MessageBoxW(0, text, APP_TITLE, 0x40)
    except Exception:
        print(text)


def show_touch_keyboard():
    """Ask Windows to show its touch keyboard for the focused text field."""
    if not IS_WIN:
        return
    path = os.path.join(os.environ.get("CommonProgramFiles", r"C:\Program Files\Common Files"),
                        "microsoft shared", "ink", "TabTip.exe")
    try:
        # ShellExecute asks Windows to open the touch keyboard in the user session.
        result = ctypes.windll.shell32.ShellExecuteW(None, "open", path, None, None, 1)
        if result > 32:
            return
    except Exception:
        pass
    if os.path.isfile(path):
        subprocess.Popen([path], close_fds=True)


# ----------------------------------------------------------------------------
# Custom "glass" widgets
# ----------------------------------------------------------------------------
def round_rect(cv, x1, y1, x2, y2, r, **kw):
    pts = [x1 + r, y1, x2 - r, y1, x2, y1, x2, y1 + r, x2, y2 - r, x2, y2,
           x2 - r, y2, x1 + r, y2, x1, y2, x1, y2 - r, x1, y1 + r, x1, y1]
    return cv.create_polygon(pts, smooth=True, **kw)


class Card(tk.Frame):
    """Rounded, bordered 'frosted' panel. Put children in .body"""

    def __init__(self, parent, fill=CARD, radius=16, pad=(18, 14)):
        super().__init__(parent, bg=BG)
        self.fill, self.radius = fill, S(radius)
        self.cv = tk.Canvas(self, bg=BG, highlightthickness=0, bd=0)
        self.cv.place(x=0, y=0, relwidth=1, relheight=1)
        self.body = tk.Frame(self, bg=fill)
        self.body.pack(fill="both", expand=True, padx=S(pad[0]), pady=S(pad[1]))
        self.bind("<Configure>", self._draw)

    def _draw(self, _e=None):
        self.cv.delete("all")
        w, h = self.winfo_width(), self.winfo_height()
        if w < 4 or h < 4:
            return
        round_rect(self.cv, 1, 1, w - 2, h - 2, self.radius, fill=self.fill, outline=BORDER, width=1)
        # soft highlight along the top edge for a glassy feel
        self.cv.create_line(self.radius, 2, w - self.radius, 2, fill="#3a4370")


class Btn(tk.Canvas):
    def __init__(self, parent, text, command=None, primary=False, min_w=0, danger=False):
        self._font = tkfont.Font(font=F(13, True))
        self.text, self.command, self.primary = text, command, primary
        self.base = ACCENT if primary else BTN
        self.hover = ACCENT_HI if primary else BTN_HI
        self.fg = BG if primary else (BAD if danger else TEXT)
        self.min_w = S(min_w)
        self.cur = self.base
        super().__init__(parent, width=self._calc_w(), height=S(34), bg=parent["bg"],
                         highlightthickness=0, cursor="hand2")
        self.bind("<Enter>", lambda e: self._paint(self.hover))
        self.bind("<Leave>", lambda e: self._paint(self.base))
        self.bind("<Button-1>", self._click)
        self._paint(self.base)

    def _calc_w(self):
        return max(self._font.measure(self.text) + S(30), self.min_w)

    def _paint(self, color):
        self.cur = color
        self.delete("all")
        w, h = int(self["width"]), int(self["height"])
        round_rect(self, 1, 1, w - 1, h - 1, S(10), fill=color, outline="")
        self.create_text(w // 2, h // 2, text=self.text, fill=self.fg, font=F(13, True))

    def set_text(self, text):
        self.text = text
        self.configure(width=self._calc_w())
        self._paint(self.cur)

    def _click(self, _e):
        if self.command:
            self.command()


class Dropdown(Btn):
    def __init__(self, parent, options, value, command=None, min_w=150):
        self.options = list(options)
        self.value = value
        self.on_change = command
        super().__init__(parent, "", self._open, min_w=min_w)
        self.set(value)

    def set(self, value):
        self.value = value
        label = dict(self.options).get(value, str(value))
        self.set_text(label + "   ▾")

    def _open(self):
        m = tk.Menu(self, tearoff=0, bg=CARD_HI, fg=TEXT, activebackground=ACCENT,
                    activeforeground=BG, bd=0, font=F(13))
        for val, label in self.options:
            m.add_command(label=label, command=lambda v=val: self._pick(v))
        try:
            m.tk_popup(self.winfo_rootx(), self.winfo_rooty() + self.winfo_height())
        finally:
            m.grab_release()

    def _pick(self, v):
        self.set(v)
        if self.on_change:
            self.on_change(v)


class Toggle(tk.Canvas):
    def __init__(self, parent, value, command=None):
        super().__init__(parent, width=S(46), height=S(26), bg=parent["bg"],
                         highlightthickness=0, cursor="hand2")
        self.value, self.command = bool(value), command
        self.bind("<Button-1>", self._click)
        self._paint()

    def _paint(self):
        self.delete("all")
        w, h = S(46), S(26)
        round_rect(self, 1, 1, w - 1, h - 1, h // 2, fill=ACCENT if self.value else BTN, outline="")
        r = h // 2 - S(4)
        cx = w - h // 2 if self.value else h // 2
        self.create_oval(cx - r, h // 2 - r, cx + r, h // 2 + r, fill="#ffffff", outline="")

    def set(self, v):
        self.value = bool(v)
        self._paint()

    def _click(self, _e):
        self.value = not self.value
        self._paint()
        if self.command:
            self.command(self.value)


class Slider(tk.Canvas):
    def __init__(self, parent, lo, hi, value, step, command=None, width=260):
        self.lo, self.hi, self.step, self.command = lo, hi, step, command
        self.value = min(max(value, lo), hi)
        super().__init__(parent, width=S(width), height=S(28), bg=parent["bg"],
                         highlightthickness=0, cursor="hand2")
        self.bind("<Button-1>", self._drag)
        self.bind("<B1-Motion>", self._drag)
        self.bind("<Configure>", lambda e: self._paint())
        self._paint()

    def _paint(self):
        self.delete("all")
        w, h = int(self.winfo_width() or self["width"]), S(28)
        pad = S(12)
        frac = (self.value - self.lo) / (self.hi - self.lo)
        x = pad + frac * (w - 2 * pad)
        y = h // 2
        self.create_line(pad, y, w - pad, y, fill=BTN_HI, width=S(6), capstyle="round")
        self.create_line(pad, y, max(pad, x), y, fill=ACCENT, width=S(6), capstyle="round")
        r = S(9)
        self.create_oval(x - r, y - r, x + r, y + r, fill="#ffffff", outline=ACCENT, width=S(2))

    def _drag(self, e):
        w, pad = int(self.winfo_width()), S(12)
        frac = min(max((e.x - pad) / max(1, w - 2 * pad), 0.0), 1.0)
        raw = self.lo + frac * (self.hi - self.lo)
        v = round(self.lo + round((raw - self.lo) / self.step) * self.step, 4)
        v = min(max(v, self.lo), self.hi)
        if v != self.value:
            self.value = v
            self._paint()
            if self.command:
                self.command(v)


class ScrollFrame(tk.Frame):
    def __init__(self, parent):
        super().__init__(parent, bg=BG)
        self.cv = tk.Canvas(self, bg=BG, highlightthickness=0, bd=0)
        self.cv.pack(fill="both", expand=True)
        self.inner = tk.Frame(self.cv, bg=BG)
        self.win = self.cv.create_window(0, 0, window=self.inner, anchor="nw")
        self.inner.bind("<Configure>", lambda e: self.cv.configure(scrollregion=self.cv.bbox("all")))
        self.cv.bind("<Configure>", lambda e: self.cv.itemconfigure(self.win, width=e.width))
        self.cv.bind("<Enter>", lambda e: self.cv.bind_all("<MouseWheel>", self._wheel))
        self.cv.bind("<Leave>", lambda e: self.cv.unbind_all("<MouseWheel>"))

    def _wheel(self, e):
        if self.inner.winfo_height() > self.cv.winfo_height():
            self.cv.yview_scroll(int(-e.delta / 120) * 2, "units")


def label(parent, text, size=13, bold=False, color=TEXT, **kw):
    return tk.Label(parent, text=text, bg=parent["bg"], fg=color, font=F(size, bold),
                    anchor="w", justify="left", **kw)


# ----------------------------------------------------------------------------
# Application window
# ----------------------------------------------------------------------------
STICK_CHOICES = [("left", "Left stick"), ("right", "Right stick"), ("none", "Off")]
PAGES = [("dash", "◈   Dashboard"), ("mouse", "⌖   Mouse"),
         ("buttons", "⌨   Buttons"), ("settings", "⚙   Settings")]


class App:
    def __init__(self, root, cfg, engine, minimized=False):
        self.root, self.cfg, self.engine = root, cfg, engine
        cfg._root = root
        self.page = None
        self.live = None
        self.learning = None
        self.tray = None
        self.nav = {}

        root.title(APP_TITLE)
        root.configure(bg=BG)
        root.geometry("%dx%d" % (S(1080), S(720)))
        root.minsize(S(940), S(620))
        root.protocol("WM_DELETE_WINDOW", self.on_close)
        self.glass_ok = apply_glass(root, cfg["glass"])

        root.columnconfigure(1, weight=1)
        root.rowconfigure(0, weight=1)
        self.build_sidebar()
        self.content = tk.Frame(root, bg=BG)
        self.content.grid(row=0, column=1, sticky="nsew", padx=(0, S(16)), pady=S(16))
        self.show("dash")
        root.bind_all("<Button-1>", self._text_field_click, add="+")
        self.start_tray()
        self.ui_tick()
        if minimized:
            root.after(50, self.hide_window)

    # -- chrome -------------------------------------------------------------
    def _text_field_click(self, event):
        if not self.cfg["touch_keyboard"]:
            return
        if isinstance(event.widget, (tk.Entry, tk.Text)):
            # Run after Tk has moved focus to the field.
            self.root.after(180, show_touch_keyboard)

    def build_sidebar(self):
        side = Card(self.root, radius=18, pad=(14, 16))
        side.grid(row=0, column=0, sticky="ns", padx=S(16), pady=S(16))
        b = side.body
        b.configure(width=S(190))
        label(b, "PAD / PC", 11, True, ACCENT).pack(fill="x", pady=(S(2), S(5)))
        label(b, "Controller\nControl", 22, True).pack(fill="x")
        label(b, "A gamepad for your desktop", 11, False, MUTED).pack(fill="x", pady=(S(6), S(22)))
        for key, text in PAGES:
            item = tk.Label(b, text=text, bg=CARD, fg=MUTED, font=F(14, True), anchor="w",
                            padx=S(12), pady=S(10), cursor="hand2")
            item.pack(fill="x", pady=S(2))
            item.bind("<Button-1>", lambda e, k=key: self.show(k))
            item.bind("<Enter>", lambda e, k=key: self.nav[k].configure(bg=CARD_HI) if k != self.page else None)
            item.bind("<Leave>", lambda e, k=key: self.nav[k].configure(bg=CARD) if k != self.page else None)
            self.nav[key] = item
        tk.Frame(b, bg=CARD).pack(fill="both", expand=True)
        row = tk.Frame(b, bg=CARD)
        row.pack(fill="x", side="bottom")
        self.dot = tk.Canvas(row, width=S(12), height=S(12), bg=CARD, highlightthickness=0)
        self.dot.pack(side="left")
        self.dot_item = self.dot.create_oval(1, 1, S(11), S(11), fill=BAD, outline="")
        self.side_status = label(row, "No controller", 12, False, MUTED)
        self.side_status.pack(side="left", padx=S(8))

    def show(self, key):
        self.cancel_learn()
        self.page, self.live = key, None
        for k, item in self.nav.items():
            item.configure(bg=ACCENT_DIM if k == key else CARD, fg=TEXT if k == key else MUTED)
        for w in self.content.winfo_children():
            w.destroy()
        getattr(self, "page_" + key)()

    def header(self, title, sub):
        h = tk.Frame(self.content, bg=BG)
        h.pack(fill="x", pady=(S(5), S(20)))
        label(h, title, 28, True).pack(fill="x")
        label(h, sub, 12, False, MUTED).pack(fill="x", pady=(S(4), 0))
        label(h, sub, 12, False, MUTED).pack(fill="x")

    # -- settings rows -----------------------------------------------------------
    def row(self, card, title, desc=None):
        r = tk.Frame(card.body, bg=card.fill)
        r.pack(fill="x", pady=S(7))
        left = tk.Frame(r, bg=card.fill)
        left.pack(side="left", fill="x", expand=True)
        label(left, title, 14, True).pack(fill="x")
        if desc:
            tk.Label(left, text=desc, bg=card.fill, fg=MUTED, font=F(11), anchor="w",
                     justify="left", wraplength=S(330)).pack(fill="x")
        right = tk.Frame(r, bg=card.fill)
        right.pack(side="right")
        return right

    def slider_row(self, card, title, desc, key, lo, hi, step, fmt):
        right = self.row(card, title, desc)
        val = label(right, fmt(self.cfg[key]), 13, True, ACCENT)
        val.configure(width=9, anchor="e")

        def changed(v):
            self.cfg.set(key, v)
            val.configure(text=fmt(v))
        Slider(right, lo, hi, self.cfg[key], step, changed, width=240).pack(side="left")
        val.pack(side="left", padx=(S(8), 0))

    def toggle_row(self, card, title, desc, key, after=None):
        right = self.row(card, title, desc)

        def changed(v):
            self.cfg.set(key, v)
            if after:
                after(v)
        Toggle(right, self.cfg[key], changed).pack()

    def choice_row(self, card, title, desc, key, options):
        right = self.row(card, title, desc)
        Dropdown(right, options, self.cfg[key], lambda v: self.cfg.set(key, v), min_w=150).pack()

    # -- pages ---------------------------------------------------------------------
    def page_dash(self):
        self.header("Dashboard", "Live view of your controller")
        top = tk.Frame(self.content, bg=BG)
        top.pack(fill="x")
        top.columnconfigure((0, 1), weight=1, uniform="a")

        c1 = Card(top)
        c1.grid(row=0, column=0, sticky="nsew", padx=(0, S(8)))
        label(c1.body, "Controller", 12, False, MUTED).pack(fill="x")
        name = label(c1.body, "Searching...", 17, True)
        name.pack(fill="x", pady=(S(2), S(10)))
        en = tk.Frame(c1.body, bg=CARD)
        en.pack(fill="x", pady=S(4))
        label(en, "Control PC with controller", 13, True).pack(side="left")
        tg = Toggle(en, self.engine.enabled, lambda v: self.engine.set_enabled(v))
        tg.pack(side="right")
        dev_row = tk.Frame(c1.body, bg=CARD)
        dev_row.pack(fill="x", pady=(S(10), 0))
        label(dev_row, "Device", 13, True).pack(side="left")
        dd = Dropdown(dev_row, [(0, "Controller 1")], 0, None, min_w=150)
        dd.pack(side="right")

        c2 = Card(top)
        c2.grid(row=0, column=1, sticky="nsew", padx=(S(8), 0))
        label(c2.body, "Live input", 12, False, MUTED).pack(fill="x")
        cv = tk.Canvas(c2.body, width=S(290), height=S(130), bg=CARD, highlightthickness=0)
        cv.pack(pady=S(6))
        R = S(46)
        centers = [(S(80), S(62)), (S(210), S(62))]
        dots = []
        for (cx, cy), nm in zip(centers, ("L", "R")):
            cv.create_oval(cx - R, cy - R, cx + R, cy + R, outline=BORDER, width=S(2), fill=FIELD)
            cv.create_text(cx, cy + R + S(14), text=nm, fill=MUTED, font=F(11, True))
            dots.append(cv.create_oval(cx - S(8), cy - S(8), cx + S(8), cy + S(8), fill=ACCENT, outline=""))
        pressed = label(c2.body, "Nothing pressed", 12, False, MUTED)
        pressed.pack(fill="x")

        c3 = Card(self.content)
        c3.pack(fill="x", pady=(S(14), 0))
        label(c3.body, "Quick settings", 12, False, MUTED).pack(fill="x")
        self.slider_row(c3, "Cursor speed", "How fast the pointer moves at full tilt", "mouse_speed",
                        100, 4000, 50, lambda v: "%d px/s" % v)
        self.slider_row(c3, "Scroll speed", "Lines per second at full tilt", "scroll_speed",
                        1, 40, 1, lambda v: "%d /s" % v)

        self.live = {"name": name, "tg": tg, "dd": dd, "cv": cv, "dots": dots, "centers": centers,
                     "R": R, "pressed": pressed, "devs": None}

    def page_mouse(self):
        self.header("Mouse", "Tune the cursor and scrolling feel")
        sf = ScrollFrame(self.content)
        sf.pack(fill="both", expand=True)
        c1 = Card(sf.inner)
        c1.pack(fill="x", pady=(0, S(12)))
        label(c1.body, "Cursor", 12, False, MUTED).pack(fill="x")
        self.choice_row(c1, "Cursor stick", "Which stick moves the pointer", "mouse_stick", STICK_CHOICES)
        self.slider_row(c1, "Speed", "Pixels per second at full tilt", "mouse_speed", 100, 4000, 50,
                        lambda v: "%d px/s" % v)
        self.slider_row(c1, "Response curve", "1 = linear. Higher = finer control near the center",
                        "curve", 1.0, 3.0, 0.1, lambda v: "%.1f" % v)
        self.slider_row(c1, "Dead zone", "Ignore small stick drift", "deadzone", 0.0, 0.5, 0.01,
                        lambda v: "%d %%" % round(v * 100))
        self.slider_row(c1, "Slow-mode factor", "Cursor speed while a 'Slow cursor' button is held",
                        "precision_factor", 0.05, 1.0, 0.05, lambda v: "x%.2f" % v)
        self.slider_row(c1, "Fast-mode factor", "Cursor speed while a 'Fast cursor' button is held",
                        "boost_factor", 1.0, 4.0, 0.1, lambda v: "x%.1f" % v)
        self.toggle_row(c1, "Invert vertical", "Push up to move the cursor down", "invert_mouse_y")
        c2 = Card(sf.inner)
        c2.pack(fill="x", pady=(0, S(12)))
        label(c2.body, "Scrolling", 12, False, MUTED).pack(fill="x")
        self.choice_row(c2, "Scroll stick", "Which stick scrolls pages", "scroll_stick", STICK_CHOICES)
        self.slider_row(c2, "Scroll speed", "Lines per second at full tilt", "scroll_speed", 1, 40, 1,
                        lambda v: "%d /s" % v)
        self.toggle_row(c2, "Reverse direction", "Natural scrolling", "invert_scroll")
        c3 = Card(sf.inner)
        c3.pack(fill="x")
        label(c3.body, "Button repeat", 12, False, MUTED).pack(fill="x")
        self.slider_row(c3, "Repeat delay", "Hold time before volume / key buttons repeat",
                        "repeat_delay", 0.1, 1.0, 0.05, lambda v: "%.2f s" % v)
        self.slider_row(c3, "Repeat rate", "Time between repeats", "repeat_rate", 0.02, 0.3, 0.01,
                        lambda v: "%.2f s" % v)

    def page_buttons(self):
        self.header("Buttons", "Choose what each controller button does")
        bar = tk.Frame(self.content, bg=BG)
        bar.pack(fill="x", pady=(0, S(10)))
        Btn(bar, "+  Add binding", self.add_binding, primary=True).pack(side="left")
        Btn(bar, "Restore default buttons", self.reset_bindings).pack(side="left", padx=S(8))
        label(bar, "Click a button name, then press the button on your controller",
              11, False, MUTED).pack(side="right")
        self.bind_scroll = ScrollFrame(self.content)
        self.bind_scroll.pack(fill="both", expand=True)
        self.fill_bindings()
        hint = label(self.content, "Shortcuts: ctrl+c, alt+tab, win+d, f11, enter, esc, space, left ...",
                     11, False, MUTED)
        hint.pack(fill="x", pady=(S(8), 0))

    def fill_bindings(self):
        for w in self.bind_scroll.inner.winfo_children():
            w.destroy()
        for b in self.cfg["bindings"]:
            self.binding_row(self.bind_scroll.inner, b)

    def binding_row(self, parent, b):
        card = Card(parent, radius=12, pad=(12, 8))
        card.pack(fill="x", pady=S(4))
        r = card.body
        inp = Btn(r, input_name(b["input"]), None, min_w=150)
        inp.command = lambda: self.learn(b, inp)
        inp.pack(side="left")
        label(r, "→", 16, True, MUTED).pack(side="left", padx=S(12))
        entry_var = tk.StringVar(value=b.get("key", ""))
        entry = tk.Entry(r, textvariable=entry_var, width=16, bg=FIELD, fg=TEXT, insertbackground=TEXT,
                         relief="flat", font=F(13), highlightthickness=1, highlightbackground=BORDER,
                         highlightcolor=ACCENT)

        def key_changed(*_):
            b["key"] = entry_var.get()
            ok = bool(parse_combo(b["key"])) or not b["key"].strip()
            entry.configure(highlightbackground=BORDER if ok else BAD)
            self.cfg.save_soon()
        entry_var.trace_add("write", key_changed)

        def action_changed(v):
            b["action"] = v
            if v == "key":
                entry.pack(side="left", padx=(S(10), 0), ipady=S(5))
            else:
                entry.pack_forget()
            self.cfg.save_soon()
        dd = Dropdown(r, ACTIONS, b["action"], action_changed, min_w=250)
        dd.pack(side="left")
        if b["action"] == "key":
            entry.pack(side="left", padx=(S(10), 0), ipady=S(5))
        Btn(r, "✕", lambda: self.delete_binding(b), min_w=40, danger=True).pack(side="right")

    def add_binding(self):
        self.cfg["bindings"].append({"input": "none", "action": "none", "key": ""})
        self.cfg.save_soon()
        self.fill_bindings()

    def delete_binding(self, b):
        self.cfg["bindings"][:] = [x for x in self.cfg["bindings"] if x is not b]
        self.cfg.save_soon()
        self.fill_bindings()

    def reset_bindings(self):
        if messagebox.askyesno(APP_TITLE, "Restore the default button layout?"):
            self.cfg["bindings"][:] = copy.deepcopy(DEFAULTS["bindings"])
            self.cfg.save_soon()
            self.fill_bindings()

    def learn(self, b, btn):
        self.cancel_learn()
        btn.set_text("Press a button...")
        self.learning = (b, btn)

        def got(inp):
            self.root.after(0, lambda: finish(inp))

        def finish(inp):
            if self.learning is None or self.learning[1] is not btn:
                return
            self.learning = None
            b["input"] = inp
            self.cfg.save_soon()
            try:
                btn.set_text(input_name(inp))
            except tk.TclError:
                pass
        self.engine.start_learn(got)
        self.root.after(8000, lambda: self.learning and self.learning[1] is btn and self.cancel_learn())

    def cancel_learn(self):
        self.engine.cancel_learn()
        if self.learning is not None:
            b, btn = self.learning
            self.learning = None
            try:
                btn.set_text(input_name(b["input"]))
            except tk.TclError:
                pass

    def page_settings(self):
        self.header("Settings", "App behavior and appearance")
        c = Card(self.content)
        c.pack(fill="x")

        def startup_changed(v):
            try:
                set_startup(v)
            except Exception as exc:
                messagebox.showerror(APP_TITLE, "Could not change startup setting:\n%s" % exc)
        right = self.row(c, "Start with Windows", "Launch hidden in the tray when you sign in")
        Toggle(right, startup_enabled(), startup_changed).pack()
        self.toggle_row(c, "Keep running when the window is closed",
                        "Closing the window sends the app to the tray instead of quitting",
                        "keep_running")
        self.toggle_row(c, "Show touch keyboard on text fields",
                        "Open the Windows touch keyboard when you tap a text box",
                        "touch_keyboard")
        self.toggle_row(c, "Glass blur effect", "Frosted-glass window background (Windows 10/11)",
                        "glass", self.glass_changed)
        right = self.row(c, "Reset everything", "Restore all settings and buttons to defaults")
        Btn(right, "Reset to defaults", self.reset_all, danger=True).pack()
        c2 = Card(self.content)
        c2.pack(fill="x", pady=(S(14), 0))
        label(c2.body, "About", 12, False, MUTED).pack(fill="x")
        label(c2.body, "Settings are saved automatically to:", 12, False, MUTED).pack(fill="x", pady=(S(4), 0))
        label(c2.body, CONFIG_PATH, 11, False, TEXT).pack(fill="x")
        label(c2.body, "Button names assume an Xbox layout. Use 'Learn' on the Buttons page for other pads.",
              11, False, MUTED).pack(fill="x", pady=(S(8), 0))

    def glass_changed(self, v):
        ok = apply_glass(self.root, v)
        if v and not ok and IS_WIN:
            messagebox.showinfo(APP_TITLE, "Glass effect is not available on this system.")

    def reset_all(self):
        if messagebox.askyesno(APP_TITLE, "Reset all settings and buttons to defaults?"):
            self.cfg.reset()
            self.engine.set_enabled(True)
            apply_glass(self.root, self.cfg["glass"])
            self.show("settings")

    # -- live updates -----------------------------------------------------------------
    def ui_tick(self):
        st = self.engine.state
        ok = st["connected"]
        self.dot.itemconfigure(self.dot_item, fill=(GOOD if ok and self.engine.enabled else
                                                     "#fbbf24" if ok else BAD))
        text = ("Paused" if ok and not self.engine.enabled else "Connected") if ok else "No controller"
        if st.get("error"):
            text = "Engine error"
        self.side_status.configure(text=text)
        lv = self.live
        if lv and self.page == "dash":
            lv["name"].configure(text=st["name"] if ok else (st["error"] or "No controller detected"))
            if lv["tg"].value != self.engine.enabled:
                lv["tg"].set(self.engine.enabled)
            devs = tuple(st["devices"])
            if devs != lv["devs"]:
                lv["devs"] = devs
                opts = [(i, n) for i, n in enumerate(devs)] or [(0, "None found")]
                lv["dd"].options = opts
                lv["dd"].on_change = lambda v: self.cfg.set("device", v)
                lv["dd"].set(min(int(self.cfg["device"]), len(opts) - 1))
            for i, dot in enumerate(lv["dots"]):
                cx, cy = lv["centers"][i]
                x, y = st["sticks"][i * 2], st["sticks"][i * 2 + 1]
                px, py = cx + x * lv["R"] * 0.8, cy + y * lv["R"] * 0.8
                r = S(8)
                lv["cv"].coords(dot, px - r, py - r, px + r, py + r)
            names = [input_name(p) for p in sorted(st["pressed"])]
            lv["pressed"].configure(text=("Pressed: " + ", ".join(names)) if names else "Nothing pressed",
                                    fg=TEXT if names else MUTED)
        self.root.after(40, self.ui_tick)

    # -- tray / lifecycle ---------------------------------------------------------------
    def start_tray(self):
        try:
            import pystray
            from PIL import Image, ImageDraw
        except Exception:
            return
        try:
            img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
            d = ImageDraw.Draw(img)
            d.rounded_rectangle((2, 2, 62, 62), 16, fill=(124, 140, 255, 255))
            d.rounded_rectangle((12, 22, 52, 46), 10, fill=(10, 12, 20, 255))
            d.ellipse((18, 28, 26, 36), fill=(255, 255, 255, 255))
            d.ellipse((38, 28, 46, 36), fill=(124, 140, 255, 255))
            after = self.root.after
            menu = pystray.Menu(
                pystray.MenuItem("Open", lambda: after(0, self.show_window), default=True),
                pystray.MenuItem("Control enabled", lambda: after(0, lambda: self.engine.set_enabled(not self.engine.enabled)),
                                 checked=lambda item: self.engine.enabled),
                pystray.MenuItem("Quit", lambda: after(0, self.quit)))
            self.tray = pystray.Icon(APP_NAME, img, APP_TITLE, menu)
            self.tray.run_detached()
        except Exception:
            self.tray = None

    def hide_window(self):
        if self.tray:
            self.root.withdraw()
        else:
            self.root.iconify()

    def show_window(self):
        self.root.deiconify()
        self.root.lift()
        self.root.focus_force()

    def on_close(self):
        if self.tray and self.cfg["keep_running"]:
            self.root.withdraw()
        else:
            self.quit()

    def quit(self):
        self.engine.alive = False
        self.engine.release_all()
        self.cfg.save()
        if self.tray:
            try:
                self.tray.stop()
            except Exception:
                pass
        self.root.destroy()


# ----------------------------------------------------------------------------
# Entry point
# ----------------------------------------------------------------------------
def main():
    global SCALE
    args = [a.lower() for a in sys.argv[1:]]
    if "--startup-on" in args:
        set_startup(True)
        popup("Enabled: Controller Control will start with Windows (hidden in the tray).")
        return
    if "--startup-off" in args:
        set_startup(False)
        popup("Disabled: Controller Control will no longer start with Windows.")
        return

    if IS_WIN:
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(1)
        except Exception:
            try:
                ctypes.windll.user32.SetProcessDPIAware()
            except Exception:
                pass
        mutex = ctypes.windll.kernel32.CreateMutexW(None, False, "Global\\ControllerToPC_single")
        if ctypes.windll.kernel32.GetLastError() == 183:
            popup("Controller Control is already running (check the system tray).")
            return

    root = tk.Tk()
    try:
        SCALE = max(1.0, root.winfo_fpixels("1i") / 96.0)
    except Exception:
        SCALE = 1.0
    cfg = Config().load()
    engine = Engine(cfg)
    engine.start()
    App(root, cfg, engine, minimized="--minimized" in args)
    root.mainloop()


if __name__ == "__main__":
    main()
