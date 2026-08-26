# 全新 Tauri 2 应用，而非 fork pot-desktop 改造

需求最初提出"可用 pot-desktop 代码库改造"。评估后决定从零构建一个 Tauri 2.x 应用，仅借鉴 pot 已验证的实现机制（`selection` crate 跨软件取词、`screenshots` 截屏、Windows.Media.Ocr 系统识别、鼠标定位弹窗、剪贴板轮询监听）。理由：

1. 本项目只用 pot 约 20% 的能力（单一 AI 翻译服务、仅 Windows 平台），裁剪其 23 个翻译服务/TTS/生词本/插件系统/外部调用 HTTP 服务/跨平台支持的成本高于重写。
2. pot 基于 Tauri 1.x，官方已停止维护；fork 将继承过时框架，后续升级成本高。
3. pot 为 GPL-3.0，整体派生要求本项目同样以 GPL-3.0 开源发布；全新实现（借鉴思路与公开机制，不复制代码）不受此约束。

## Considered Options

- Fork pot-desktop 裁剪改造 —— 上手快，但继承开源义务与过时技术栈（否决）。
- Electron 全新开发 —— 包体与内存开销数倍于 Tauri，截屏/取词仍需原生辅助（否决）。
