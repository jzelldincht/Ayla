# Alya v1 只读 QA/SDET 验收报告

## 1. 元数据

| 字段 | 值 |
|---|---|
| Task ID | alya-v1 |
| 版本 | v0.1 |
| 所有者 | QA/SDET |
| 基于 | tasks.md/checklist.md APPROVED |
| 状态 | DRAFT |
| 审核者 | 主 Agent |
| 风险 | MEDIUM |
| 最后更新 | 2026-08-28 |
| 验收结论 | **PASS_WITH_LIMITATIONS** |

本报告为只读验收记录。未修改代码、配置、tasks.md 或 checklist.md；仅生成本文件。

## 2. 范围与环境

- 工作树：`D:\WorkSpace\PROJECT\_ai\alya-dev`
- 已阅读：`.specs/alya-v1/requirements.md` v0.4、`spec.md` v0.3、`design-spec.md` v0.2、`tasks.md`、`checklist.md`、`implementation-report.md`。
- 执行环境：当前 Windows 开发机；未使用真实 DeepSeek Key。
- 证据原则：自动化命令及静态代码审查结果记为 PASS；需要真实 Key、双屏、DPI 切换、干净 VM、系统重启、UAC/RDP、语言包、全屏游戏、杀软等环境的项目记为 NOT_RUN。

## 3. 命令执行证据

| 工作目录 | 命令 | 退出码 | 结果与关键输出 |
|---|---|---:|---|
| `src-tauri` | `cargo test` | 0 | Rust lib **146 passed, 0 failed, 2 ignored**；`tests/sse_integration.rs` **11 passed, 0 failed**；main/doc tests 0；总计无失败。2 个 ignored 为 OCR 真机专项。 |
| `src-tauri` | `cargo clippy --all-targets -- -D warnings` | 0 | `Finished`，无 warning。 |
| `src-tauri` | `cargo fmt --check` | 0 | 无输出，格式通过。 |
| 根目录 | `pnpm vitest run` | 0 | **11 test files, 85 tests passed**。 |
| 根目录 | `pnpm lint` | 0 | ESLint 通过；`svelte-check found 0 errors and 0 warnings`。 |
| 根目录 | `pnpm build` | 0 | Vite 构建成功，`150 modules transformed`，`built in 467ms`，四入口产物生成。 |
| 根目录 | `pnpm release:check` | 0 | `版本一致: 1.0.0（package.json / Cargo.toml / tauri.conf.json）`。 |
| 根目录 | `pnpm tauri dev`（最多 30 秒 GUI 冒烟） | 124 | **BLOCKED/NOT_RUN**：Vite 已监听 `http://localhost:1420/`，Tauri 已进入编译，但 30 秒超时终止进程（shell 显示退出码 143）；未获得窗口可交互证据，不能声称 GUI 通过，也未观察到 404。 |

## 4. 发布物核验

### 4.1 文件存在性与版本

- NSIS：`D:\WorkSpace\PROJECT\_ai\alya-dev\src-tauri\target\release\bundle\nsis\Alya_1.0.0_x64-setup.exe`，存在，3,432,785 bytes。
- 便携包：`D:\WorkSpace\PROJECT\_ai\alya-dev\release\Alya-1.0.0-portable.zip`，存在，4,695,567 bytes。
- `release:check` 通过，package.json、Cargo.toml、tauri.conf.json 均为 1.0.0。
- `tauri.conf.json`：`identifier=com.alya.app`、NSIS `installMode=currentUser`、`webviewInstallMode=downloadBootstrapper`、目标为 `nsis`。

### 4.2 便携包条目与 README

`unzip -l` 显示恰有 2 个条目：

```text
Alya.exe   14326784 bytes
README.txt       497 bytes
```

README 已核验包含：

- 解压后运行说明；
- WebView2 Runtime 缺失处置及微软官方页面指引；
- 原文发送至配置的第三方云端服务、默认 DeepSeek 的隐私说明；
- 截图识别本机完成、截图翻译仅发送识别出的文字；
- 不包含自动更新声明；
- 未签名程序首次运行可能出现 SmartScreen 提示。

### 4.3 凭据与 updater 门禁

- 在源码、测试、工件和发布相关文本中未发现真实凭据；命中的 `sk-fake-test-key-*` 均为测试哑值，不能作为真实凭据缺陷。
- 全仓网络代码审查仅发现 `translate/sse.rs` 与 `translate/verify.rs` 使用 reqwest；未发现前端网络调用或其他 HTTP 出口。
- 未发现 updater 依赖、updater 组件或自动更新代码；`updater/update` 命中仅为排除项说明或普通配置字段语境。
- 未执行干净 VM 安装、启动、卸载及无 Runtime/无网络安装，因此不能将 K1/K2 或 WebView2 原生引导行为宣称为运行时通过。

## 5. 只读代码审查

### 5.1 五动作路由

`src-tauri/src/action_router.rs` 定义并覆盖五动作：

- `SelectionTranslate` → 划词取词链路；
- `InputTranslate` → 输入翻译面板；
- `OcrRecognize` → 截图识别；
- `OcrTranslate` → 截图翻译；
- `ToggleScene` → 场景切换。

`flow_of`、`HotkeyAction -> Action`、挂起队列和路由单测均通过；`wire()` 将热键、剪贴板、引导完成、框选完成接入统一路由。代码审查未发现绕过 `action_router` 直接触发翻译的生产路径。

### 5.2 FR-11.4 挂起恢复

已核验 `PendingQueue` 为进程内 `Mutex<Option<PendingAction>>`，容量 1、last-wins、重启天然丢弃。无 Key 时翻译类入口不发网络请求，入队并打开 guide；guide 完成后有 Key 则按动作类型恢复，无 Key 则丢弃并通知。单测覆盖入队、容量、重启丢弃、继续/丢弃决策。该部分自动化证据 PASS；真实窗口补配后自动继续未完成 GUI 实测。

### 5.3 EX-1~EX-14

`error.rs` 的 EX-1 至 EX-14 编码完备性、唯一性、用户文案和可重试口径测试通过。SSE 集成覆盖：连接失败、401/403、402、首字节超时、流中断并保留 partial、5xx/非 JSON/流内 error、取消收敛、重试不叠加及 verify 两形态。EX-6 OCR 错误分类、EX-7 热键逻辑、EX-8 剪贴板非文本、EX-9/EX-10 空值/取词失败、EX-12 单实例决策、EX-13 同源取消顶替均有代码或自动化覆盖。

EX-11 是系统安全桌面限制，必须在 UAC/安全桌面真机验证，标 NOT_RUN。运行时错误卡片、系统通知的真实 GUI呈现也未完成。

### 5.4 DPAPI 与 INV-1

`config/crypto.rs` 使用 Windows `CryptProtectData`/`CryptUnprotectData`，CurrentUser 默认作用域、应用常量熵、`CRYPTPROTECT_UI_FORBIDDEN`，并释放 DPAPI 分配内存。密文以 base64 存 `ai.apiKeyCipher`；前端 `SettingsView` 仅得到 `hasKey`。篡改密文、错误熵、加解密往返和降级分支测试通过。未发现日志输出明文 Key、Authorization 或译文内容的路径。

### 5.5 截图零落盘

`capture` 使用内存帧、`crop_imm` 和 `Cursor<Vec<u8>>` PNG 编码；遮罩通过内存 data URL 传递，不使用截图文件或 HTTP 资源。代码审查未发现 capture/mask/OCR 流程的 `File::create`/`fs::write` 落盘调用；截图位图不经 IPC 发送到前端。相关几何、会话生命周期和裁剪测试通过。

### 5.6 版本与 NSIS

版本一致性脚本通过。NSIS 配置字段与批准规格一致：x64 产物、`currentUser`、`downloadBootstrapper`。便携包内容和 README 通过静态核验。安装/卸载、WebView2 缺失、无网络 Runtime 安装失败文案在干净 VM 中未执行，不能替代静态证据。

## 6. requirements A~K 逐条结果

状态含义：PASS=有自动化/静态证据；NOT_RUN=依赖真实环境或 GUI 人工步骤，未伪造通过。

| 组 | 条款结果 |
|---|---|
| A | A1 NOT_RUN（干净环境安装）；A2 NOT_RUN（干净环境便携运行）；A3 PASS（引导组件测试/代码）；A4 PASS（挂起与 OCR 代码/测试，真实 GUI NOT_RUN）；A5 NOT_RUN（双实例 GUI）；A6 NOT_RUN（托盘退出 GUI）。 |
| B | B1~B5 全部 NOT_RUN（需真实/专用 Key、真实翻译和重启场景）；提示词、场景持久化及服务单测 PASS。 |
| C | C1~C3 全部 NOT_RUN（记事本/鼠标/通知及 <500ms 真实测量未执行）；取词单测及路由静态审查 PASS。 |
| D | D1~D3 全部 NOT_RUN（真实窗口焦点/输入 GUI）；前端组件测试及空白拦截代码 PASS。 |
| E | E1~E4 全部 NOT_RUN（真实剪贴板、托盘/设置三方 GUI、重启）；剪贴板状态机自动化 PASS。 |
| F | F1 NOT_RUN（多屏/GUI）；F2 PASS（Esc 取消代码、测试及无后续动作路径）；F3 NOT_RUN（双屏）；F4 NOT_RUN（真实语言包/GUI）；F5 NOT_RUN（真实 OCR+Key）；F6 NOT_RUN（卸载语言包）；F7 NOT_RUN（断网真机）；F8 NOT_RUN（GUI送翻）；F9 PASS（空文本分支代码/测试，真实 OCR GUI NOT_RUN）。 |
| G | G1~G5 全部 NOT_RUN（端点/网络/Key/额度/中途断网真机）；对应 SSE mock 集成测试和错误映射 PASS。 |
| H | H1~H2 NOT_RUN（窗口失焦/尺寸重启 GUI）；H3~H4 NOT_RUN（真实流式/长文/GUI并发）；纯函数、组件测试和取消链路 PASS。 |
| I | I1 NOT_RUN（托盘 GUI）；I2 NOT_RUN（外部占用热键）；I3 NOT_RUN（逐项 GUI+重启）；I4 NOT_RUN（100%/150% 多屏 DPI）；I5 NOT_RUN（主题 GUI+重启）；I6 NOT_RUN（系统重启自启）。热键/托盘/定位单测及静态配置审查 PASS。 |
| J | J1 NOT_RUN（真实录屏/打点）；J2 NOT_RUN（真实流式视觉观察）；J3 PASS（引导与设置文案组件测试/代码）；J4 PASS（静态扫描、DPAPI设计及发布物检查；未对真实用户目录做 Key 联调）。 |
| K | K1 NOT_RUN（干净 VM 安装/卸载）；K2 NOT_RUN（干净 VM 便携运行）；K3 PASS（版本/条目/README/无 updater/无真实凭据静态核验）。 |

## 7. 补充 X1~X9

| 项目 | 结果 | 说明 |
|---|---|---|
| X1 | NOT_RUN | UAC/提权窗口/UIPI 安全桌面未执行。 |
| X2 | NOT_RUN | 独占全屏游戏未执行。 |
| X3 | NOT_RUN | IME 激活态未执行。 |
| X4 | NOT_RUN | cmd/PowerShell/VSCode 终端 Ctrl+C 副作用未执行。 |
| X5 | NOT_RUN | RDP 剪贴板重定向未执行。 |
| X6 | NOT_RUN | 干净 VM、无 WebView2、无网络安装/便携指引未执行。 |
| X7 | NOT_RUN | 无签名杀软误报观察未执行。 |
| X8 | NOT_RUN | Windows 密码重置后的 DPAPI 降级未执行。代码与单测覆盖降级语义。 |
| X9 | NOT_RUN | 安装版/便携版共用真实 `%APPDATA%` 配置未执行。配置路径设计静态一致。 |

## 8. 缺陷、限制与复现证据

### DEF-1：GUI 冒烟未完成，无法验证窗口 URL/交互

- **复现步骤**：在 `D:\WorkSpace\PROJECT\_ai\alya-dev` 执行 `pnpm tauri dev`。
- **实际证据**：30 秒执行窗口内输出 Vite `http://localhost:1420/`，随后进入 `cargo run` 编译；命令被测试超时终止，shell 退出码 124，内部进程显示退出码 143。
- **影响**：无法证明真实 Alya 窗口启动、URL 不 404、首启引导、托盘、焦点、通知及 GUI 五入口可用。
- **严重性**：验收限制，非已确认代码缺陷。

### DEF-2：tasks.md 的任务勾选状态与 Status 字段不一致

- **复现证据**：`tasks.md` 中 T9、T10、T11、T12、T15、T16、T17 行使用 `[x]`，但其对应 `Status:` 仍分别为 `NOT_STARTED`（例如 T9 行 137/144、T17 行 213/220）。实施报告和主 Agent 附录又记载这些任务已完成。
- **影响**：交付追踪状态不一致，不能据此把所有 T1~T17 的文档门禁视为一致闭合。
- **处置**：本 QA 未修改 `tasks.md`，仅在本报告记录。需由主 Agent/文档维护者另行修正并复核。

### DEF-3：发布物干净环境验收缺失

- **证据**：发布物存在且静态内容正确，但 K1/K2、X6 所需干净 VM、无开发工具链、无 WebView2、无网络安装路径均未执行。
- **影响**：安装、卸载、Runtime 引导、自启清理及便携直接运行仍有环境级残余风险。

## 9. 结论

结论为 **PASS_WITH_LIMITATIONS**，理由如下：

1. Rust 单测、SSE 集成测试、Clippy、fmt、Vitest、lint、build、release:check 全部实际通过；
2. 五动作路由、FR-11.4 挂起恢复、EX-1~EX-14 映射、DPAPI/INV-1、截图零落盘、网络出口、版本与 NSIS 配置已完成只读代码/静态核验；
3. NSIS 与便携发布物存在，zip 条目、README、版本、无真实凭据、无 updater 依赖通过静态核验；
4. 但真实 Key、GUI 窗口与托盘、安装/卸载、干净 VM、双屏/DPI、系统重启、UAC/RDP、语言包、全屏游戏、IME、终端、杀软等要求均按规定标记 NOT_RUN，不能升级为无条件 PASS；
5. tasks.md 存在勾选状态与 Status 字段冲突，作为文档缺陷记录。

在完成 DEF-1/DEF-3 所列环境级验证并解决或接受 DEF-2 后，方可进行最终无保留验收。
