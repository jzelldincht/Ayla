# -*- coding: utf-8 -*-
"""截图工具：DPI 感知 + 窗口枚举 + 区域抓屏（媒体材料采集用，非项目代码）。"""
import ctypes, sys, time
from ctypes import wintypes

u = ctypes.windll.user32
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # PER_MONITOR_AWARE_V2

WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

def list_windows(visible_only=True):
    out = []
    def cb(h, l):
        if visible_only and not u.IsWindowVisible(h):
            return True
        buf = ctypes.create_unicode_buffer(256)
        u.GetWindowTextW(h, buf, 256)
        cls = ctypes.create_unicode_buffer(256)
        u.GetClassNameW(h, cls, 256)
        r = wintypes.RECT()
        u.GetWindowRect(h, ctypes.byref(r))
        out.append({
            'hwnd': h, 'title': buf.value, 'class': cls.value,
            'rect': (r.left, r.top, r.right, r.bottom),
            'w': r.right - r.left, 'h': r.bottom - r.top,
        })
        return True
    u.EnumWindows(WNDENUMPROC(cb), 0)
    return out

def find(title_sub, exact=False):
    for w in list_windows():
        if (w['title'] == title_sub) if exact else (title_sub in w['title']):
            if w['w'] > 60 and w['h'] > 60:
                return w
    return None

def grab(rect, path):
    from PIL import ImageGrab
    img = ImageGrab.grab(bbox=rect, all_screens=True)
    img.save(path)
    return img.size

def grab_window(title_sub, path, pad=0):
    w = find(title_sub)
    if not w:
        return None
    x0, y0, x1, y1 = w['rect']
    size = grab((x0 - pad, y0 - pad, x1 + pad, y1 + pad), path)
    return {'window': w, 'size': size}

def screen_info():
    import ctypes
    gsm = u.GetSystemMetrics
    return {'screen': (gsm(0), gsm(1)), 'virt': (gsm(76), gsm(77)), 'primary': (gsm(0), gsm(1))}

if __name__ == '__main__':
    cmd = sys.argv[1] if len(sys.argv) > 1 else 'list'
    if cmd == 'list':
        for w in list_windows():
            print(w['hwnd'], repr(w['title'][:44]), w['class'], w['rect'])
    elif cmd == 'screen':
        print(screen_info())
    elif cmd == 'grab':
        # grab <title_substr> <out.png> [pad]
        pad = int(sys.argv[4]) if len(sys.argv) > 4 else 0
        r = grab_window(sys.argv[2], sys.argv[3], pad)
        print('grabbed' if r else 'NOT FOUND', r)
    elif cmd == 'grabfull':
        print(grab((0, 0, u.GetSystemMetrics(0), u.GetSystemMetrics(1)), sys.argv[2]))
