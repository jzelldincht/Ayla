# Alya — Privacy Policy / 隐私政策

**Last updated: 2026-09-18 ｜ 生效日期：2026-09-18**
**Applies to: Alya for Windows (Microsoft Store / standalone / portable)**

---

## English

### Summary
Alya is a local desktop application. It has no accounts, no analytics, and no servers of its own. The only data that leaves your PC is the text you explicitly choose to translate — and it goes directly to the API service **you** configured.

### 1. What Alya processes and where it goes

| Data | Where it goes | Notes |
|---|---|---|
| Text you translate (selection, typed/pasted input, or clipboard content when clipboard monitoring is ON) | Sent over HTTPS to the **API service you configured** (e.g., DeepSeek or any OpenAI-compatible endpoint) | Alya does not operate any proxy or relay server. The request goes from your PC to your provider. |
| Text recognized from a screenshot (Alt+S / Alt+D) | OCR runs **locally** using the built-in Windows OCR engine. Only the **recognized text** is sent to your configured API service | The screenshot image itself is never uploaded, never written to disk, and is discarded after recognition. |
| Your API key | Stored **encrypted on your device** using Windows DPAPI (tied to your Windows user account) | The key is never displayed in plain text by the app and is never transmitted anywhere except to your configured API endpoint as part of the request. |

### 2. What Alya does NOT do
- No telemetry, analytics, crash reporting, or usage statistics.
- No advertising, no ad SDKs, no third-party trackers.
- No user accounts, no cloud storage, no server-side history of your translations.
- No access to your files, other applications' data, or your screen — with one deliberate exception: when **you** press the capture hotkey (Alt+S / Alt+D), Alya takes a screenshot **at that moment** so you can select a region. This happens only on your keypress.

### 3. Local data on your device
Alya stores its settings (including your encrypted API key) and local log files in your Windows user profile:
- `%APPDATA%\com.alya.app\config.json` — settings; the API key is DPAPI-encrypted
- `%LOCALAPPDATA%\com.alya.app\logs\` — local diagnostic logs (kept on your device only)

Deleting the application and these folders removes all local data. Since there is no account, nothing needs to be deleted remotely.

### 4. Clipboard monitoring (optional, off by default)
When you enable clipboard monitoring, Alya reads new clipboard text **on your device** to send it for translation. It is off by default and can be turned off at any time from the panel's top bar or the tray menu.

### 5. Third-party services
Translation is performed by the API provider **you** choose and configure. Their handling of the text you translate is governed by **their** privacy policy and terms. Alya has no relationship with, and receives no data from, these providers.

### 6. Children
Alya is a general-purpose productivity tool and is not directed at children. It collects no personal information.

### 7. Changes
If this policy changes, the updated version will be published at the same location with a new "last updated" date.

### 8. Contact
Questions about this policy: **<在此填写你的支持邮箱 / your support email>**

---

## 简体中文

### 摘要
Alya 是本地桌面应用：没有账号体系、没有统计分析、没有自建服务器。离开你电脑的数据只有你主动选择翻译的文本，且直接发送给你**自己配置**的 API 服务。

### 1. 数据处理与去向

| 数据 | 去向 | 说明 |
|---|---|---|
| 你翻译的文本（划词 / 输入 / 开启剪贴板监听后的复制内容） | 经 HTTPS 直接发送至**你配置的 API 服务**（如 DeepSeek 或任意 OpenAI 兼容端点） | Alya 不运营任何中转或代理服务器；请求由你的电脑直达你选择的服务商 |
| 截图识别出的文字（Alt+S / Alt+D） | 文字识别 100% **在本机**用 Windows 系统 OCR 完成；仅**识别结果**发送至你配置的服务 | 截图图像本身永不上传、不落盘，识别后即丢弃 |
| 你的 API Key | 使用 Windows DPAPI **加密存储在本机**（绑定当前 Windows 账户） | 应用任何界面都不回显明文；除作为请求头发往你配置的端点外，不向任何地方传输 |

### 2. Alya 不做的事
- 无遥测、无统计、无崩溃上报、无使用行为追踪；
- 无广告、无广告 SDK、无第三方跟踪器；
- 无账号、无云端存储、无服务端翻译历史；
- 不访问你的文件、其他应用数据或屏幕——唯一例外：当你**主动按下**截图热键（Alt+S / Alt+D）时，Alya 于该时刻截屏供你框选，仅在按键瞬间发生。

### 3. 本机存储
设置（含加密后的 API Key）与本地日志位于你的 Windows 用户目录：
- `%APPDATA%\com.alya.app\config.json` — 设置项（API Key 为 DPAPI 密文）
- `%LOCALAPPDATA%\com.alya.app\logs\` — 本地诊断日志（仅存本机）

删除应用及上述目录即清除全部本地数据；因无账号体系，无需远程注销。

### 4. 剪贴板监听（可选，默认关闭）
开启后，Alya 在本机读取新的剪贴板文本并发送翻译。该功能默认关闭，可随时在面板顶栏或托盘关闭。

### 5. 第三方服务
翻译由**你自行选择并配置**的 API 服务商完成，其对文本的处理适用该服务商的隐私政策与条款。Alya 与服务商之间不存在数据代理关系。

### 6. 未成年人
Alya 为通用效率工具，不面向儿童，不收集任何个人信息。

### 7. 政策变更
如有变更，将在同一位置发布更新版本并标注新的生效日期。

### 8. 联系方式
隐私相关问题请联系：**<在此填写你的支持邮箱>**
