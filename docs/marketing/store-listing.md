# Microsoft Store 上架材料 — Alya

> 适用：Partner Center 提交页各字段直接复制粘贴。英文为商店主语言（全球），中文附于其后供国内资料备份。

---

## 1. Product name / 产品名称

- **Product name**: `Alya`
- **Package/Display 备选**（若名称被占用）: `Alya Translate`

---

## 2. Short description（≤ 100 字符，商店摘要）

**EN (94 chars):**
```
Instant AI translation anywhere on Windows. Hotkeys, OCR capture, your own API key.
```

**ZH（≤ 40 字）:**
```
Windows 任意界面一键翻译：划词、输入、截图取字，自带 API 密钥。
```

---

## 3. Description / 详细描述

### English

```
Alya is a lightweight translation panel for Windows. It lives in your system tray and appears only when you need it — translate without leaving the app you are working in.

FOUR WAYS TO TRANSLATE
• Selection (Alt+Q) — select text in any app and the translated panel pops up next to your cursor.
• Input (Alt+E) — open a small writing panel; type or paste, press Enter, read the translation.
• Clipboard (optional) — turn on clipboard monitoring and every new copy gets translated automatically.
• Capture (Alt+D / Alt+S) — for text you cannot select: subtitles, game UI, images, scanned PDFs. Drag a box around it and Alya recognizes the text and translates it.

PRIVACY BY DESIGN
• Screenshots never leave your PC: text is extracted locally with the built-in Windows OCR engine. Only the recognized characters are sent for translation.
• Bring your own API key: Alya does not proxy your traffic and has no accounts, no telemetry, and no usage tracking.
• Your API key is encrypted locally with Windows DPAPI and is never displayed by the app.
• Clipboard monitoring is off by default and can be toggled anytime.

BUILT FOR DAILY USE
• Mutual or directed mode: let Alya detect the translation direction, or pin a target language — and switch languages on the fly, re-translating instantly.
• Streaming or staged output: watch the translation as it is written, or wait for long texts to arrive as a finished block with an estimated time.
• The panel remembers: position, size, and the source-text height you set. The translation area grows with the content, so long texts never squeeze your original text.
• Works on multi-monitor and high-DPI setups with per-monitor scaling; the panel stays inside your work area and snaps neatly to the taskbar edge.
• Hotkey conflicts are handled gracefully: if a shortcut is taken by another app, Alya rolls back to the previous binding and tells you where to fix it.
• Light, dark, and follow-system themes — including the native window frame.
• Nine UI languages: English, 简体中文, 繁體中文, 日本語, 한국어, Français, Deutsch, Español, Русский.

HOW IT WORKS
Alya connects to any OpenAI-compatible API service that you configure (DeepSeek by default, but any compatible endpoint works). You pay your provider directly for what you use — no subscription to Alya.

Try it: install, press Alt+E, and type a sentence.
```

### 简体中文（备档）

```
Alya 是常驻 Windows 托盘的轻量翻译面板，需要时随叫随到，无需离开当前工作窗口。

四种翻译方式
• 划词（Alt+Q）：任意应用中选中文字，面板在光标旁即时给出译文。
• 输入（Alt+E）：呼出输入面板，输入或粘贴后回车即译。
• 剪贴板（可选）：开启监听后，每次复制的新内容自动翻译。
• 截图（Alt+D / Alt+S）：字幕、游戏界面、图片、扫描件等无法选中的文字，拖框即识别并翻译。

隐私优先
• 截图不离开本机：文字用系统自带 Windows OCR 在本地提取，仅把识别结果发送翻译。
• 自带 API 密钥：不经中间服务器、无账号体系、无遥测统计。
• 密钥以 Windows DPAPI 本地加密，任何界面不回显。
• 剪贴板监听默认关闭，可随时在面板顶栏或托盘关闭。

日常体验
• 互译 / 定向双场景，目标语言可随时切换并立即重译。
• 全部流式或分档呈现（长文等待整段输出并给出预估时间）。
• 面板记忆位置、尺寸与输入框高度；译文区随内容自动展开。
• 多显示器、高 DPI 下自动适配，不越出工作区。
• 热键冲突自动回退并提示，录制式改键。
• 亮 / 暗 / 跟随系统主题，九语种界面。

工作原理
Alya 连接你自己配置的 OpenAI 兼容 API（默认 DeepSeek）。费用由你直付服务商，Alya 不收订阅费。
```

---

## 4. Features / 功能要点（商店 "Features" 列表，每条一行）

```
• Selection / input / clipboard / screenshot translation in one panel
• Screenshot OCR runs locally — images are never uploaded
• Bring your own OpenAI-compatible API key (no subscription)
• Mutual or directed translation with instant language switching
• Streaming and staged output modes with ETA for long texts
• Remembers panel position, size, and source-text height
• Multi-monitor and per-monitor DPI aware
• Hotkey conflict recovery with recording-based rebinding
• Light / dark / system themes
• Nine UI languages
• Tray-resident, single instance, optional launch at startup
```

---

## 5. Screenshots（已按商店要求生成 1920×1080）

| 文件 | 建议标题（商店可加说明文字） |
|---|---|
| `screenshots/alya-store-01-input.png` | Translate as you type |
| `screenshots/alya-store-02-ocr.png` | Translate text you cannot select |
| `screenshots/alya-store-03-settings.png` | Bring your own OpenAI-compatible API key |
| `screenshots/alya-store-04-dark.png` | Dark and light, your layout |
| `screenshots/alya-store-05-onboarding.png` | Ready in under a minute |

**Store logo**: `screenshots/alya-store-logo-300.png`（300×300 PNG）

> 说明：截图中的 API 地址与模型名为**示例值**，非任何真实配置；界面为英文环境实机截取。

---

## 6. System requirements / 系统要求

- **OS**: Windows 10 version 1809 (build 17763) or later; Windows 11
- **Architecture**: x64
- **Runtime**: Microsoft Edge WebView2 Runtime（Win11 内置；Win10 一般随系统更新存在，安装包会声明依赖）
- **Memory / Disk**: 约 60 MB 磁盘占用，常驻内存低于同类的 Electron 类工具
- **Network**: 仅翻译功能需要（连接用户配置的 API 服务）

---

## 7. Category & metadata / 分类与元数据

- **Category**: Productivity（生产力）
- **Subcategory / Tags**: Translator, Utilities, OCR, Productivity
- **Age rating**: 3+ / Everyone（问答问卷全部选“无”即可：无暴力、无色情、无赌博、无用户生成内容、无广告）
- **支持的语言（界面）**: en, zh-CN, zh-TW, ja, ko, fr, de, es, ru
- **Support contact**: <在此填写你的支持邮箱>
- **Privacy policy URL**: <在此填写隐私政策页面地址，见 privacy-policy.md>
- **Website**: <可选，官网或 GitHub 仓库地址>

---

## 8. What's new / 发行说明（V1.0.0）

```
First release.
• Four translation entries: selection, input, clipboard, screenshot
• Local OCR capture translation (screenshots never leave your PC)
• Bring-your-own-key: any OpenAI-compatible API
• Mutual / directed modes, streaming / staged output
• Panel geometry memory: position, size, source-text height
• Nine UI languages, light / dark / system themes
```

---

## 9. 提交前检查清单

- [ ] Partner Center 个人账户已通过身份验证（$19 一次性）
- [ ] 名称 `Alya` 未被占用（若占用改用 `Alya Translate`）
- [ ] 隐私政策页面已上线且可公开访问（商店硬性要求）
- [ ] 5 张截图与 300×300 徽标已上传（规格：PNG，≥1366×768，本文档已按 1920×1080 生成）
- [ ] 年龄分级问卷已完成
- [ ] 支持邮箱可收信
- [ ] 提交的是 MSIX 包（打包流程见上架实施说明），包身份与商店预留名称一致
- [ ] 首次认证若被拒：核对“功能声明与实际一致”“隐私政策可访问”“无崩溃”
