# -*- coding: utf-8 -*-
"""放置/调整窗口位置（物理像素）。用法: place_window.py <title> x y w h"""
import sys, ctypes, time
u = ctypes.windll.user32
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
title = sys.argv[1]
x, y, w, h = (int(v) for v in sys.argv[2:6])
hwnd = None
import ctypes.wintypes as wt
def cb(h, l):
    global hwnd
    buf = ctypes.create_unicode_buffer(256); u.GetWindowTextW(h, buf, 256)
    if title in buf.value and u.IsWindowVisible(h):
        hwnd = h; return False
    return True
u.EnumWindows(ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)(cb), 0)
if not hwnd:
    print('NOT FOUND'); sys.exit(1)
u.SetWindowPos(hwnd, 0, x, y, w, h, 0x0040)  # SWP_SHOWWINDOW
print('placed', hwnd)
