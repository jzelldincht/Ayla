# -*- coding: utf-8 -*-
"""桌面输入自动化：鼠标/键盘/前台切换（媒体采集用）。"""
import ctypes, time
u = ctypes.windll.user32
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

VK = {'alt': 0x12, 'ctrl': 0x11, 'shift': 0x10, 'win': 0x5B,
      'esc': 0x1B, 'enter': 0x0D, 'tab': 0x09, 'space': 0x20,
      'q': 0x51, 'e': 0x45, 's': 0x53, 'd': 0x44, 'v': 0x56, 'a': 0x41, 'c': 0x43, 'w': 0x57}

def _key(vk, up=False):
    u.keybd_event(vk, 0, 0x0002 if up else 0, 0)

def tap(vk, hold=0.03):
    _key(vk); time.sleep(hold); _key(vk, True)

def combo(*keys, gap=0.05):
    for k in keys[:-1]:
        _key(VK.get(k, k)); time.sleep(gap)
    time.sleep(gap)
    last = VK.get(keys[-1], keys[-1])
    tap(last)
    time.sleep(gap)
    for k in reversed(keys[:-1]):
        _key(VK.get(k, k), True); time.sleep(0.02)
    time.sleep(0.15)

def click(x, y, button='left'):
    u.SetCursorPos(int(x), int(y)); time.sleep(0.12)
    if button == 'right':
        u.mouse_event(0x0008, 0, 0, 0, 0); time.sleep(0.05); u.mouse_event(0x0010, 0, 0, 0, 0)
    else:
        u.mouse_event(0x0002, 0, 0, 0, 0); time.sleep(0.05); u.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.3)

def drag(x0, y0, x1, y1, steps=18):
    u.SetCursorPos(int(x0), int(y0)); time.sleep(0.25)
    u.mouse_event(0x0002, 0, 0, 0, 0); time.sleep(0.2)
    for i in range(1, steps + 1):
        u.SetCursorPos(int(x0 + (x1-x0)*i/steps), int(y0 + (y1-y0)*i/steps)); time.sleep(0.02)
    time.sleep(0.2); u.mouse_event(0x0004, 0, 0, 0, 0); time.sleep(0.4)

def foreground(hwnd):
    u.ShowWindow(hwnd, 5)
    try:
        u.SetForegroundWindow(hwnd)
    except Exception:
        pass
    time.sleep(0.35)

def cursor_at(x, y):
    u.SetCursorPos(int(x), int(y)); time.sleep(0.1)

def foreground_force(hwnd):
    """绕过前台锁定：AttachThreadInput + SetForegroundWindow + BringWindowToTop。"""
    k32 = ctypes.windll.kernel32
    fg = u.GetForegroundWindow()
    tid_fg = u.GetWindowThreadProcessId(fg, None)
    tid_me = k32.GetCurrentThreadId()
    u.AttachThreadInput(tid_me, tid_fg, True)
    u.ShowWindow(hwnd, 9)      # SW_RESTORE
    u.SetForegroundWindow(hwnd)
    u.BringWindowToTop(hwnd)
    u.AttachThreadInput(tid_me, tid_fg, False)
    time.sleep(0.5)
