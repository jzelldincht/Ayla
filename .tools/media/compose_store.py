# -*- coding: utf-8 -*-
"""Store 截图合成：1920×1080 画布 + 品牌底色 + 说明文案 + 应用截图（含敏感字段替换）。"""
import io, os
from PIL import Image, ImageDraw, ImageFont

W, H = 1920, 1080
OUT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', 'docs', 'marketing', 'screenshots'))
os.makedirs(OUT, exist_ok=True)

FONT_B = 'C:/Windows/Fonts/segoeuib.ttf'   # Segoe UI Bold
FONT_R = 'C:/Windows/Fonts/segoeui.ttf'    # Segoe UI Regular
ACCENT = (37, 99, 235)
ACCENT_D = (23, 55, 130)
BG_TOP = (13, 26, 58)
BG_BOT = (23, 46, 96)

def grad_bg():
    img = Image.new('RGB', (W, H))
    d = ImageDraw.Draw(img)
    for y in range(H):
        t = y / H
        c = tuple(int(BG_TOP[i] + (BG_BOT[i] - BG_TOP[i]) * t) for i in range(3))
        d.line([(0, y), (W, y)], fill=c)
    # 柔光圆
    glow = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([W - 900, -300, W + 300, 700], fill=(37, 99, 235, 60))
    gd.ellipse([-300, H - 500, 600, H + 300], fill=(37, 99, 235, 40))
    return Image.alpha_composite(img.convert('RGBA'), glow).convert('RGB')

def shadowed(shot, radius=18, pad=44, blur=28):
    """给截图加圆角 + 投影，返回 RGBA 图。"""
    from PIL import ImageFilter
    shot = shot.convert('RGBA')
    mask = Image.new('L', shot.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, shot.size[0] - 1, shot.size[1] - 1], radius=radius, fill=255)
    card = Image.new('RGBA', shot.size, (0, 0, 0, 0))
    card.paste(shot, (0, 0), mask)
    canvas = Image.new('RGBA', (shot.size[0] + pad * 2, shot.size[1] + pad * 2), (0, 0, 0, 0))
    sh = Image.new('RGBA', canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle([pad + 6, pad + 14, pad + shot.size[0] + 6, pad + shot.size[1] + 14],
                                         radius=radius, fill=(0, 0, 0, 170))
    sh = sh.filter(ImageFilter.GaussianBlur(blur))
    canvas = Image.alpha_composite(canvas, sh)
    canvas.paste(card, (pad, pad), card)
    return canvas

def compose(shot_path, title, subtitle, out_name, mode='right', scale=None, y_align='center'):
    bg = grad_bg().convert('RGBA')
    shot = Image.open(shot_path)
    if scale:
        shot = shot.resize((int(shot.width * scale), int(shot.height * scale)), Image.LANCZOS)
    card = shadowed(shot)
    d = ImageDraw.Draw(bg)
    d.text((110, 118), title, font=ImageFont.truetype(FONT_B, 62), fill=(255, 255, 255))
    d.text((112, 205), subtitle, font=ImageFont.truetype(FONT_R, 30), fill=(178, 196, 232))
    d.text((110, H - 96), 'Alya  ·  instant translation for Windows', font=ImageFont.truetype(FONT_R, 24),
           fill=(140, 160, 205))
    if mode == 'right':
        x = W - card.width - 60 if card.width < W - 700 else (W - card.width) // 2
        y = (H - card.height) // 2 + 20
    else:
        x = (W - card.width) // 2
        y = (H - card.height) // 2 + 30
    if y_align == 'bottom':
        y = H - card.height + 10
        x = W - card.width - 40
    bg.alpha_composite(card, (max(x, 640), max(y, 0)))
    out = os.path.join(OUT, out_name)
    bg.convert('RGB').save(out)
    print('saved', out, bg.size)

def redact_fields(src, patches, out):
    """覆盖输入框内容为示例值（隐藏真实配置）。patches: [(x0,y0,x1,y1,text)]"""
    im = Image.open(src).convert('RGB')
    d = ImageDraw.Draw(im)
    f = ImageFont.truetype(FONT_R, 30)
    for (x0, y0, x1, y1, text) in patches:
        d.rounded_rectangle([x0, y0, x1, y1], radius=8, fill=(243, 244, 246))
        d.text((x0 + 18, y0 + (y1 - y0) // 2 - 18), text, font=f, fill=(31, 41, 55))
    im.save(out)
    print('redacted ->', out)
    return out

MEDIA = os.path.dirname(os.path.abspath(__file__))

# 0) AI 设置页去敏（真实 endpoint/model → 示例值）
redacted_ai = redact_fields(
    os.path.join(MEDIA, 'raw_en_03_settings_ai.png'),
    [(1008, 214, 1488, 272, 'https://api.openai.com/v1'),
     (1008, 340, 1488, 398, 'gpt-4o-mini')],
    os.path.join(MEDIA, '_en_settings_ai_redacted.png'))

# 1) 输入翻译（主图）
compose(os.path.join(MEDIA, 'raw_en_01_input_mutual.png'),
        'Translate as you type',
        'Press Alt+E, type or paste text, get the translation instantly.',
        'alya-store-01-input.png', scale=0.63)

# 2) 截图翻译
compose(os.path.join(MEDIA, 'raw_en_02_ocr.png'),
        'Translate text you cannot select',
        'Press Alt+D and drag a box: OCR runs locally, only text is sent.',
        'alya-store-02-ocr.png', scale=0.63)

# 3) 自己的 API Key
compose(redacted_ai,
        'Bring your own OpenAI-compatible API key',
        'Point Alya at any compatible service and pick the model you like.',
        'alya-store-03-settings.png', scale=0.60, y_align='bottom')

# 4) 深色主题
compose(os.path.join(MEDIA, 'raw_en_07_dark_panel.png'),
        'Dark and light, your layout',
        'The panel remembers where you put it and how big you made it.',
        'alya-store-04-dark.png', scale=0.63)

# 5) 首次设置
compose(os.path.join(MEDIA, 'raw_en_08_onb_welcome.png'),
        'Ready in under a minute',
        'A short first-run guide sets up your API key path.',
        'alya-store-05-onboarding.png', scale=0.78)

# 6) 商店方形徽标（300×300，取自应用图标）
logo_src = os.path.abspath(os.path.join(MEDIA, '..', '..', 'src-tauri', 'icons', 'Square310x310Logo.png'))
logo = Image.open(logo_src).convert('RGBA').resize((300, 300), Image.LANCZOS)
logo.save(os.path.join(OUT, 'alya-store-logo-300.png'))
print('logo saved')
