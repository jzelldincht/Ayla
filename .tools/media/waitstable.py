# -*- coding: utf-8 -*-
"""轮询截屏直到区域视觉稳定（缩略图灰度比对，容忍光标闪烁）。"""
import sys, time
sys.path.insert(0, '.')
from win import find, grab
from PIL import ImageGrab

def thumb(rect):
    img = ImageGrab.grab(bbox=rect, all_screens=True).convert('L').resize((48, 32))
    return list(img.getdata())

def diff(a, b):
    return sum(abs(x - y) for x, y in zip(a, b)) / len(a)

title, out = sys.argv[1], sys.argv[2]
timeout = float(sys.argv[3]) if len(sys.argv) > 3 else 30
prev, stable, start, w = None, 0, time.time(), None
while time.time() - start < timeout:
    w = find(title)
    if not w:
        time.sleep(0.4); continue
    cur = thumb(w['rect'])
    if prev is not None and diff(cur, prev) < 1.2:
        stable += 1
        if stable >= 2:
            break
    else:
        stable = 0
    prev = cur
    time.sleep(1.0)
print(grab(w['rect'], out) if w else ('NOT FOUND ' + title), 'stable' if stable >= 2 else 'TIMEOUT')
