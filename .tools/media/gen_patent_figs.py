# -*- coding: utf-8 -*-
"""专利附图生成：黑白线条图（白底黑线，符合专利附图惯例）。"""
import os
from PIL import Image, ImageDraw, ImageFont

OUT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', 'docs', 'patent', '附图'))
os.makedirs(OUT, exist_ok=True)
FB = 'C:/Windows/Fonts/msyhbd.ttc'   # 微软雅黑粗体（中文标注）
FR = 'C:/Windows/Fonts/msyh.ttc'
BLACK = (0, 0, 0)
W, H = 1700, 1250

def new(w=W, h=H):
    im = Image.new('RGB', (w, h), 'white')
    return im, ImageDraw.Draw(im)

def box(d, x, y, w, h, text, fs=26, bold=False, dashed=False, radius=6):
    if dashed:
        # 虚线矩形
        seg = 14
        for i in range(x, x + w, seg * 2):
            d.line([(i, y), (min(i + seg, x + w), y)], fill=BLACK, width=3)
            d.line([(i, y + h), (min(i + seg, x + w), y + h)], fill=BLACK, width=3)
        for j in range(y, y + h, seg * 2):
            d.line([(x, j), (x, min(j + seg, y + h))], fill=BLACK, width=3)
            d.line([(x + w, j), (x + w, min(j + seg, y + h))], fill=BLACK, width=3)
    else:
        d.rounded_rectangle([x, y, x + w, y + h], radius=radius, outline=BLACK, width=3)
    f = ImageFont.truetype(FB if bold else FR, fs)
    lines = text.split('\n')
    th = len(lines) * (fs + 8)
    ty = y + (h - th) // 2
    for ln in lines:
        tw = d.textlength(ln, font=f)
        d.text((x + (w - tw) / 2, ty), ln, font=f, fill=BLACK)
        ty += fs + 8

def arrow(d, x1, y1, x2, y2, width=3, label=None, fs=22):
    d.line([(x1, y1), (x2, y2)], fill=BLACK, width=width)
    import math
    ang = math.atan2(y2 - y1, x2 - x1)
    L = 16
    for s in (+1, -1):
        d.line([(x2, y2), (x2 - L * math.cos(ang - s * 0.42), y2 - L * math.sin(ang - s * 0.42))],
               fill=BLACK, width=width)
    if label:
        f = ImageFont.truetype(FR, fs)
        mx, my = (x1 + x2) / 2, (y1 + y2) / 2
        d.text((mx + 10, my - fs - 4), label, font=f, fill=BLACK)

def title(d, text, w=W):
    f = ImageFont.truetype(FB, 34)
    d.text(((w - d.textlength(text, font=f)) / 2, 26), text, font=f, fill=BLACK)

def label(d, x, y, text, fs=24):
    f = ImageFont.truetype(FR, fs)
    d.text((x, y), text, font=f, fill=BLACK)

# ---------------- 图 1：系统架构 ----------------
im, d = new()
title(d, '图 1  翻译浮窗控制系统架构')
box(d, 80, 120, 460, 150, '触发层\n全局热键（划词/输入/截图）\n剪贴板监听', fs=25)
box(d, 620, 120, 460, 150, '统一显示路径\n定位 → 尺寸 → 显示', fs=25, bold=True)
box(d, 1160, 120, 460, 150, '界面渲染层\n原文区 / 译文区 / 顶栏', fs=25)
arrow(d, 540, 195, 620, 195)
arrow(d, 1080, 195, 1160, 195)
box(d, 620, 360, 460, 130, '控制层', fs=30, bold=True)
subs = [('显示器识别\n工作区 + 缩放', 120), ('尺寸钳制\n位置相关上限', 560), ('内容测量\n目标高度计算', 1000)]
for txt, x in subs:
    box(d, x, 540, 380, 170, txt, fs=24)
    arrow(d, x + 190, 490, x + 190, 540)
box(d, 100, 540, 460, 170, '位置记忆\n逻辑坐标 + 防抖', fs=24, dashed=True)
box(d, 1140, 540, 480, 170, '用户偏好存储\n尺寸 / 输入区高度', fs=24, dashed=True)
arrow(d, 750, 400, 380, 540)
arrow(d, 950, 400, 1300, 540)
box(d, 620, 800, 460, 130, '本地配置文件（单一事实源）', fs=25)
arrow(d, 380, 710, 700, 800, label='读写')
arrow(d, 1300, 710, 1000, 800, label='读写')
box(d, 620, 1010, 460, 130, '界面只读镜像（广播同步）', fs=25)
arrow(d, 850, 930, 850, 1010)
im.save(os.path.join(OUT, 'fig1-系统架构.png'))

# ---------------- 图 2：定位与尺寸流程 ----------------
im, d = new()
title(d, '图 2  浮窗定位与尺寸自适应流程')
cx = 850
y = 100
steps = ['翻译触发事件（划词/输入/截图/剪贴板）', '获取光标位置 → 匹配显示器 → 读取工作区与缩放']
for s in steps:
    box(d, cx - 400, y, 800, 90, s, fs=25)
    arrow(d, cx, y + 90, cx, y + 140); y += 140
box(d, cx - 430, y, 860, 110, '存在历史记忆位置？', fs=26, bold=True)
arrow(d, cx + 430, y + 55, 1620, y + 55, label='否')
arrow(d, cx - 430, y + 55, 60, y + 55, label='是')
box(d, 1180, y + 110, 440, 130, '逻辑坐标 × 缩放\n钳制于工作区', fs=24)
box(d, 60, y + 110, 440, 130, '定位于工作区右侧\n垂直居中', fs=24)
y2 = y + 300
box(d, cx - 500, y2, 1000, 110, '尺寸钳制：宽 ≤ 工作区宽 − 2×边距；高 ≤ 工作区高', fs=24)
arrow(d, 1180 + 220, y + 240, cx + 100, y2); arrow(d, 280, y + 240, cx - 100, y2)
y2 += 160
box(d, cx - 500, y2, 1000, 100, '显示浮窗并渲染内容（置顶）', fs=25)
arrow(d, cx, y2 + 100, cx, y2 + 150); y2 += 150
box(d, cx - 560, y2, 1120, 110, '测量译文内容自然高 → 目标高 = max(用户偏好, 内容所需)', fs=24)
arrow(d, cx, y2 + 110, cx, y2 + 160); y2 += 160
box(d, cx - 620, y2, 1240, 110, '位置相关上限 = 工作区底边 − 当前顶边 → 钳制目标高', fs=24, bold=True)
arrow(d, cx, y2 + 110, cx, y2 + 160); y2 += 160
box(d, cx - 620, y2, 1240, 110, '施加高度；底边越界则底边钉住工作区底边、顶边上移', fs=24)
im.save(os.path.join(OUT, 'fig2-定位与尺寸流程.png'))

# ---------------- 图 3：内容自适应与豁免 ----------------
im, d = new()
title(d, '图 3  内容驱动高度自适应与偏好隔离流程')
x, y = 120, 110
box(d, x, y, 620, 90, '译文内容更新（流式增量 / 完成）', fs=25); y += 140
box(d, x, y, 620, 90, '防抖合并（120 ms）', fs=25); arrow(d, x + 310, y - 50, x + 310, y); y += 140
box(d, x, y, 620, 90, '测量内容自然高 → 计算目标窗口高', fs=25); arrow(d, x + 310, y - 50, x + 310, y); y += 140
box(d, x, y, 620, 90, '目标高 = max(用户偏好, 内容所需)', fs=25); arrow(d, x + 310, y - 50, x + 310, y); y += 140
box(d, x, y, 620, 90, '钳制：最小高 ≤ 目标高 ≤ 位置相关上限', fs=25); arrow(d, x + 310, y - 50, x + 310, y); y += 140
box(d, x - 40, y, 700, 110, '设置「待定豁免计数」→ 施加尺寸', fs=25, bold=True); arrow(d, x + 310, y - 50, x + 310, y); y += 160
box(d, x, y, 620, 90, '尺寸变更事件到达 → 消费计数 → 跳过持久化', fs=24, dashed=True)
arrow(d, x + 310, y - 50, x + 310, y)
# 右侧：用户路径
xr = 1080
box(d, xr, 110, 520, 90, '用户手动调整窗口 / 输入区高度', fs=24)
box(d, xr, 250, 520, 90, '静默防抖（400 ms）判定调整结束', fs=24)
arrow(d, xr + 260, 200, xr + 260, 250)
box(d, xr, 390, 520, 110, '逻辑尺寸写入用户偏好\n（正常持久化路径）', fs=24, bold=True)
arrow(d, xr + 260, 340, xr + 260, 390)
label(d, 1080, 560, '结论：内容驱动的临时变化不污染用户偏好；', fs=26)
label(d, 1080, 605, '用户偏好优先于内容所需（目标高取二者较大者）；', fs=26)
label(d, 1080, 650, '短内容时自动回落至偏好高度。', fs=26)
im.save(os.path.join(OUT, 'fig3-内容自适应与偏好隔离.png'))

# ---------------- 图 4：分区与贴边 ----------------
im, d = new(1700, 1000)
title(d, '图 4  浮窗分区（黄金分割）与贴边贴合状态', w=1700)
# 左：分区
lx, ly, lw, lh = 160, 120, 480, 760
d.rounded_rectangle([lx, ly, lx + lw, ly + lh], radius=12, outline=BLACK, width=3)
top_h = int(lh * 0.06)
d.line([(lx, ly + top_h), (lx + lw, ly + top_h)], fill=BLACK, width=2)
label(d, lx + 12, ly + 8, '顶栏（固定界面高度）')
gold = ly + int(lh * 0.382)
d.line([(lx, gold), (lx + lw, gold)], fill=BLACK, width=3)
d.line([(lx + lw - 90, gold - 12), (lx + lw - 30, gold - 12)], fill=BLACK, width=2)
d.line([(lx + lw - 60, gold - 12), (lx + lw - 60, gold)], fill=BLACK, width=2)
label(d, lx + 12, ly + 60, '原文编辑区')
label(d, lx + 12, gold - 40, '黄金分割点（0.382 × 浮窗高）')
label(d, lx + 12, gold + 14, '译文区（约 61.8%，随内容自适应）')
d.text((lx + lw - 260, ly - 46), '图 4a  分区', font=ImageFont.truetype(FB, 28), fill=BLACK)
# 右：贴边状态
rx, ry, rw, rh = 900, 120, 480, 760
d.line([(rx - 80, ry + rh), (rx + rw + 80, ry + rh)], fill=BLACK, width=5)   # 工作区底边（任务栏上沿）
label(d, rx - 80, ry + rh + 16, '工作区底边（任务栏上沿）')
d.rounded_rectangle([rx, ry, rx + rw, ry + rh], radius=12, outline=BLACK, width=3)
label(d, rx + 12, ry + rh - 60, '贴合状态：取消下缘绘制边距（无空带）')
d.line([(rx + 20, ry + rh), (rx + rw - 20, ry + rh - 6)], fill=BLACK, width=2)
d.text((rx + 150, ry - 46), '图 4b  贴边贴合', font=ImageFont.truetype(FB, 28), fill=BLACK)
im.save(os.path.join(OUT, 'fig4-分区与贴边状态.png'))

print('figures written to', OUT)
for f in sorted(os.listdir(OUT)):
    print(' -', f)
