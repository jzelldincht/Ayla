# -*- coding: utf-8 -*-
"""软著源程序清单生成：按固定文件顺序拼接源码，每页 50 行、连续行号，输出前 30 页与后 30 页。
用法：python gen_source_listing.py
输出：docs/copyright/源程序清单-前30页.txt / 源程序清单-后30页.txt
"""
import io, os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
OUT = os.path.join(ROOT, 'docs', 'copyright')
LINES_PER_PAGE = 50
PAGES_EACH_END = 30

# 文件顺序：后端入口与核心 → 业务模块 → 前端入口与共享 → 各窗口组件
ORDER = [
    'src-tauri/src/main.rs',
    'src-tauri/src/lib.rs',
    'src-tauri/src/action_router.rs',
    'src-tauri/src/autostart.rs',
    'src-tauri/src/error.rs',
    'src-tauri/src/error_text.rs',
    'src-tauri/src/events.rs',
    'src-tauri/src/instance.rs',
    'src-tauri/src/tray.rs',
    'src-tauri/src/ui_text.rs',
    'src-tauri/src/capture/mod.rs',
    'src-tauri/src/clipboard_monitor/mod.rs',
    'src-tauri/src/config/mod.rs',
    'src-tauri/src/config/crypto.rs',
    'src-tauri/src/hotkey/mod.rs',
    'src-tauri/src/ocr/mod.rs',
    'src-tauri/src/selection/mod.rs',
    'src-tauri/src/translate/mod.rs',
    'src-tauri/src/windows/mod.rs',
    'src-tauri/src/windows/mask.rs',
    'src-tauri/src/windows/onboarding.rs',
    'src-tauri/src/windows/settings.rs',
    'src-tauri/src/windows/translate.rs',
    'src/lib/i18n.svelte.ts',
    'src/lib/ipc.ts',
    'src/lib/types.ts',
    'src/lib/stores/index.ts',
    'src/lib/stores/settings.svelte.ts',
    'src/lib/i18n/zh-CN.ts',
    'src/lib/i18n/en.ts',
    'src/translate/App.svelte',
    'src/translate/TopBar.svelte',
    'src/translate/OriginalEditor.svelte',
    'src/translate/StreamOutput.svelte',
    'src/translate/ErrorCard.svelte',
    'src/translate/lib/autoTranslate.ts',
    'src/translate/lib/errors.ts',
    'src/translate/lib/estimate.ts',
    'src/translate/lib/panelFit.ts',
    'src/translate/lib/retranslate.ts',
    'src/translate/lib/scrollAnchor.ts',
    'src/settings/App.svelte',
    'src/settings/HotkeyRecorder.svelte',
    'src/onboarding/App.svelte',
    'src/mask/App.svelte',
]

def gather():
    """拼接为 [(相对路径, 行内容)] 的连续行序列（含文件分隔注释）。"""
    lines = []
    for rel in ORDER:
        p = os.path.join(ROOT, rel.replace('/', os.sep))
        if not os.path.exists(p):
            print('  [跳过·不存在]', rel)
            continue
        lines.append('// ==================== ' + rel + ' ====================')
        with io.open(p, encoding='utf-8', errors='ignore') as f:
            for ln in f:
                lines.append(ln.rstrip('\n').rstrip('\r'))
    return lines

def render(lines, start_line_no, out_path, first_page, last_page):
    """按 50 行/页渲染指定页区间，页内连号（全局行号）。"""
    buf = []
    total_pages = (len(lines) + LINES_PER_PAGE - 1) // LINES_PER_PAGE
    for page in range(first_page, min(last_page, total_pages) + 1):
        s = (page - 1) * LINES_PER_PAGE
        e = min(s + LINES_PER_PAGE, len(lines))
        buf.append('=' * 78)
        buf.append('Alya 智能翻译助手软件 V1.0    源程序清单    第 %d 页（共 %d 页）' % (page, total_pages))
        buf.append('=' * 78)
        for i in range(s, e):
            buf.append('%6d  %s' % (start_line_no + i, lines[i]))
        buf.append('')
    io.open(out_path, 'w', encoding='utf-8', newline='\n').write('\n'.join(buf))
    print('written', out_path, '(%d 页)' % (min(last_page, total_pages) - first_page + 1))

def main():
    lines = gather()
    total_pages = (len(lines) + LINES_PER_PAGE - 1) // LINES_PER_PAGE
    print('总行数:', len(lines), '总页数(50行/页):', total_pages)
    os.makedirs(OUT, exist_ok=True)
    render(lines, 1, os.path.join(OUT, '源程序清单-前30页.txt'), 1, PAGES_EACH_END)
    render(lines, 1, os.path.join(OUT, '源程序清单-后30页.txt'), max(1, total_pages - PAGES_EACH_END + 1), total_pages)

if __name__ == '__main__':
    main()
