# Spec：Alya v1 技术规格（架构设计）

> Task ID: alya-v1
> 版本: v0.3
> 所有者: 系统架构师
> 基于: requirements.md v0.4
> 状态: APPROVED
> 审核者: 主 Agent
> 变更记录: v0.3 = tasks 拆解裁决落盘——translation://begin 增补 renderMode 字段（C1）；translate 窗 topmost 语义改为 show 置位/失隐清除（C3）。v0.2 = 面板默认尺寸对齐 design-spec（460×340）；D1~D10 裁定经主 Agent 核准，其中 D3/D4 另经用户批准（2026-08-26）
> 风险等级: MEDIUM
> 最后更新: 2026-08-26

## 背景与目标

本文将 requirements.md v0.4（APPROVED）的全部 FR-x / EX-x 映射为可实施的技术设计与可验证架构约束。技术路线遵循 docs/adr/0001：全新 Tauri 2.x 应用，仅 Windows 10 x64+/11，借鉴 pot-desktop 已验证机制（只读参考，严禁复制其 GPL 代码）。目标对应关系：

| 目标 | 架构支撑 |
|---|---|
| G1 五大入口 | 5.6 四条关键数据流 + 5.7 核心机制 |
| G2 双场景模型 | 6.3 配置模型（持久/临时态分离）+ 5.6 各链路的场景读取点 |
| G3 OpenAI 兼容流式 | 5.7.7 SSE 客户端与取消链路 |
| G4 托盘常驻轻量形态 | 5.1 进程模型 + 5.5 窗口清单 + 5.7.9 |
| G5 凭据安全与隐私 | 5.7.8 DPAPI 模块 + 第 7 章纪律 |
| G6 发布物 | 第 8.4 打包与分发 |
| G7 性能 | 7.2 性能预算（J1 拆解）+ 5.7.7 渲染链路 |

术语严格遵循 CONTEXT.md（互译、定向、活动场景、目标语言、划词翻译、取词、输入翻译、剪贴板监听、截图识别、截图翻译、翻译面板）。

## 范围与非目标

**设计范围**：本文覆盖进程模型、窗口体系、模块划分、关键数据流、核心机制、前后端契约、配置模型、错误映射、性能预算、打包分发、测试策略概要。

**非设计项（沿用 requirements.md 第 3 章全部非目标）**：非 Windows 平台；第三方翻译平台；TTS/生词本/收藏夹；插件系统与外部 HTTP 服务端；WebDAV/云同步；自动更新；多语言界面；放大镜/像素拾取器；历史记录与翻译内容持久化；多套 AI 服务配置；源语种检测展示。

**架构级明确不做**（防止实施加戏）：
- 不引入本地语种检测库（lingua 等）——互译判向由提示词语义约定承担（FR-2.1/2.5），本地判向属于重复建设且与需求口径冲突。
- 不引入数据库（SQLite 等）——配置规模为一个 JSON 文件。
- 不自建任何监听端口 / HTTP 服务端（request.md 约束）。
- 不做窗口位置记忆（FR-9.3 只承诺尺寸记忆；面板每次跟随鼠标定位）。
- 不做 Markdown/富文本译文渲染（见裁定 D9）。
- 不引入 OCR 备用引擎；Windows.Media.Ocr 是唯一引擎。

## 当前项目现状

- 项目根 `D:\WorkSpace\PROJECT\_ai\alya` 为空工作区，尚无代码；已存在 CONTEXT.md、docs/adr/0001、`.specs/alya-v1/`（request.md v0.1、requirements.md v0.3 APPROVED、state.md v0.1、本文件）。
- 参考库 `D:\WorkSpace\PROJECT\_ai\_refs\pot-desktop`（Tauri 1.8 + React 18，GPL-3.0）：只读借鉴机制与库选型，已查阅其 Cargo.toml 依赖、window.rs 窗口编排、clipboard.rs 轮询监听、hotkey.rs 注册、system_ocr.rs、screenshot.rs。**门禁：任何文件不得出现与其源文件的实质性同构代码；借鉴限于机制描述与公开 Win32/WRT API 用法。**
- 工具链就绪：Node v24.11.0 / pnpm 10.24.0 / Rust 1.89 / cargo 1.89，平台 win32。
- 上游状态：requirements.md v0.3 已获主 Agent APPROVED；本 spec 为 DRAFT，**未经主 Agent 验收通过不得进入实施**。
- UI/UX 设计师产出 design-spec.md 与本文件并行；本文仅约定与 UI 的接口点（见 6.4），视觉细节不在本文范围。

## 需求口径与技术裁定

提示词正文以 requirements.md FR-2.5 / FR-2.6 为唯一权威源，本文不复录（防双源漂移）；实现与测试以其文件内容逐字为准（快照测试固化，任何改动触发重新评审）。

以下裁定为架构层补充口径，已经主 Agent 核准（2026-08-26，v0.2 APPROVED）；其中 D3、D4 因引入用户可见行为差异另经用户批准：

| # | 裁定 | 内容与理由 | 影响 |
|---|---|---|---|
| D1 | 超时口径拆解 | EX-4 的 30 秒解释为「连接建立 + 发出请求 + 收到响应头（首字节前）」的上限；流式开始后以「相邻数据块间隔 > 30s」判定为传输中断走 EX-5。理由：硬性 30s 总时长会在长文流式中途误杀正常请求，与 FR-10.3 整段模式"等待动效 + 预估剩余时间"矛盾；EX-4 原文"响应超过时限"支持此读法 | EX-4/EX-5、FR-10.3 |
| D2 | 剪贴板去重口径 | 按**内容比较**去重：读到与上次基线相同的文本不触发（即使剪贴板序号变化，即用户重复复制同一内容也不重复翻译）。理由：FR-5.5 字面"重复读取到相同文本不重复触发"；同时天然覆盖 FR-5.7 恢复原内容的路径 | FR-5.5/5.7 |
| D3 | 剪贴板自动翻译长度上限 | 自动监听触发时原文超过 50,000 字符则跳过翻译并以系统通知告知（常量可调，仅作用于自动链路，手动输入不限）。理由：风险清单第 6 条要求的超大文本防护；**此为新增用户可见行为，超出需求字面，须主 Agent/用户确认** | FR-5.2 |
| D4 | 冻结快照框选 | 框选期间用户看到的是**触发瞬间的屏幕快照**（定格画面），而非实时画面。理由：避免遮罩窗自身被截入、保证多步流程画面一致（业界通行做法）。用户可见语义差异：框选期间画面不实时更新 | FR-8 全组 |
| D5 | 暖窗策略 | translate 面板窗口在启动后预创建并隐藏（WebView 冷启动 300–800ms 会击穿 J1 预算）；代价为常驻约 30–70MB WebView 内存。mask 窗口首次触发后池化复用 | G7/J1、5.5 |
| D6 | 连接验证请求形态 | FR-11.1(c) 的最小验证请求：先 `GET {base}/models`（免计费、验 Key）；若端点不支持（404/405）则回退发送 `max_tokens=1` 的最小 chat 请求。模型名不在验证期强校验，错误模型名在首次翻译时以 EX-14 呈现 | FR-11.1 |
| D7 | 单实例前台握手 | 二次启动的新实例（此刻拥有前台激活权）在退出前向既有实例 PID 调用 `AllowSetForegroundWindow`，保证既有实例唤起面板能拿到焦点（满足 FR-4.1 焦点落在输入区）；PID 经应用缓存目录下的 `instance.pid` 文件传递 | FR-1.4、EX-12 |
| D8 | 应用标识符冻结候选 | identifier 取 `com.alya.app`。它决定配置目录路径、自启注册项与安装升级识别，**发布 1.0.0 前是唯一可修改窗口期**，此后冻结 | 6.3、8.4 |
| D9 | 译文区纯文本渲染 | 译文按纯文本（保留换行的增量追加）渲染，不解析 Markdown。理由：FR-10.4 要求增量追加不闪烁，纯文本可直接操作文本节点；提示词已约束"只回复译文"。用户可见差异：模型输出中的 markdown 符号将原样显示 | FR-10.4 |
| D10 | 失焦隐藏不取消翻译 | 面板因失焦隐藏时，进行中的翻译继续后台完成（用户主动要的配额消耗不浪费）；中止仅发生在 EX-13 新翻译顶替与显式取消。重新显示面板可见完整结果 | FR-9.2/9.6 |

## 架构与技术方案

### 5.1 进程模型与总体结构

单一主进程（Rust，`alya.exe`），内嵌 WebView2 多视图；WebView2 Runtime 自行派生的浏览器子进程（msedgewebview2.exe）由平台管理，不属于本项目代码。无常驻服务、无驱动、无监听端口；网络出站仅有两个出口：翻译流式请求与连接验证请求（均指向用户配置端点）。

职责分层：

- **Rust 后端**：全部系统能力（热键、剪贴板、取词、截屏、OCR、托盘、单实例、自启、加密存储）与全部业务状态的事实源（配置、活动场景、监听开关、翻译会话）。前端不直接访问系统与网络。
- **前端（每窗口一个 WebView 页面）**：纯呈现与交互转发，通过类型化 invoke 命令与事件与后端通信；不持有密钥、不直连网络（不变量 INV-1/INV-2，见 5.4）。

### 5.2 前端框架决策

| 候选 | 优势 | 劣势（相对本项目） | 结论 |
|---|---|---|---|
| React 18 + TS | 生态最大，组件库最多 | 需要 VDOM 协调层；本项目无重型组件库诉求，生态优势用不上；流式高频追加场景需额外优化（memo/refs）才能保证直接文本节点更新 | 否 |
| **Svelte 5 + TS** | 编译期框架、无 VDOM 运行时，包体与更新开销最小；细粒度响应式天然契合"增量追加、局部刷新"的流式渲染（FR-10.4）；TS 一等支持；Tauri 官方模板支持 | 生态小于 React；团队需熟悉 runes——但本项目界面复杂度低（一表单一面板一遮罩一向导），不足以需要 React 级生态 | **采纳** |
| 原生 TS | 零依赖 | 设置表单、主题、向导、事件绑定全部手写，自研响应式代码量大、回归面大，违背"克制做加法"的反面（自制轮子） | 否 |

结论：Svelte 5（runes，初始化时锁定当时最新稳定 minor）+ TypeScript + Vite，pnpm 管理。四个窗口各自独立入口页，共享一个 `lib/`（类型化 IPC 包装、stores、主题）。

### 5.3 关键依赖清单（Rust）

| crate | 用途 | 选型说明 |
|---|---|---|
| tauri 2.x | 应用框架 | features 按需最小化；窗口阴影用内置能力，不再引 window-shadows |
| tauri-plugin-global-shortcut | 热键注册（RegisterHotKey 封装） | 注册失败以 Err 返回，作为 EX-7 检测点 |
| tauri-plugin-single-instance | 单实例（FR-1.4） | 配合 D7 PID 握手 |
| tauri-plugin-autostart | 开机自启（FR-1.5） | 写 HKCU Run 项【假设 A10】 |
| tauri-plugin-notification | 系统通知 | EX-3.2/7/10、D3 等通知呈现 |
| tauri-plugin-log | 文件日志 | 7.4 日志纪律 |
| selection | 跨软件取词（模拟复制） | ADR-0001 点名机制；MIT/Apache 双许可独立 crate，非 GPL 代码复制【假设 A2】 |
| screenshots | 多显示器截屏 | 物理分辨率 RGBA 帧【假设 A5】 |
| windows（Win32/WinRT 绑定） | OCR、DPAPI、剪贴板序号、前台控制 | features：Media_Ocr、Globalization、Foundation、Storage_Streams、Graphics_Imaging、Win32_Security_Cryptography、Win32_UI_WindowsAndMessaging、Win32_Foundation |
| image | 内存裁剪与 PNG 编码 | 配合 screenshots，全程内存 |
| arboard | 剪贴板读写（备份/恢复/监听读取） | 读文本失败即视为非文本（FR-5.4 判定基础） |
| reqwest 0.12 + eventsource-stream + futures-util | SSE 流式客户端 | json/stream/rustls-tls features；不用 async-openai（其错误模型与 EX 映射不对齐） |
| tokio | 异步运行时与超时控制 | timeout 用于 D1 两级超时 |
| serde / serde_json / thiserror | 序列化与错误建模 | error.rs 建立 AppError ↔ EX 编码映射 |
| winreg | WebView2 存在性探测 | 便携版启动自检（8.4） |

版本策略：初始化任务时锁定当时最新稳定版本记入 Cargo.toml，本文不写死 patch 号。**明确不引入**：tiny_http（服务端）、updater、sql/store/fs-watch（非目标）。

### 5.4 模块划分

```
src/
  lib.rs/main.rs        应用装配：插件注册顺序、托盘初始化、单实例、启动序列
  config/
    mod.rs              Settings 结构体（6.3 schema）、加载/保存（tmp+rename 原子写）、schemaVersion 迁移桩
    crypto.rs           DPAPI 加解密（5.7.8）
  hotkey/mod.rs         热键管理服务（5.7.1）
  clipboard_monitor/    监听状态机（5.6.3）
  selection/mod.rs      取词编排（备份→注入→等待→读取→恢复，5.6.1）
  capture/mod.rs        快照会话：抓屏、按矩形裁剪、PNG 内存编码（5.7.4/5.7.5）
  ocr/mod.rs            Windows.Media.Ocr 封装（5.7.6）：语言枚举、引擎创建、识别
  translate/
    prompt.rs           场景→提示词构造（占位符字面替换，逐字基线）
    sse.rs              流式客户端（5.7.7）
    service.rs          翻译会话：generation 计数、取消、超时、错误分类（EX 映射）
  windows/
    translate.rs        面板生命周期/定位/失焦隐藏（5.5/5.7.2/5.7.3）
    mask.rs             遮罩窗池（5.7.4）
    settings.rs         设置窗（按需创建、隐藏复用）
    onboarding.rs       引导窗（首启/FR-11.4 补配复用）
  tray.rs               托盘菜单构建与状态同步（场景勾选、监听勾选实时反映 config）
  events.rs             事件 payload 类型（6.2 契约的唯一定义处）
  error.rs              AppError 枚举，携带 ex_code 字段
  action_router.rs      入口动作汇聚（热键/托盘/单实例回调 → 统一动作：selection_translate /
                        input_translate / ocr_recognize / ocr_translate / toggle_scene）
```

依赖方向规则：`action_router → 各能力模块 → config/events`；禁止反向；禁止模块间绕过 action_router 直接触发翻译（保证 EX-13 顶替逻辑只有一条路径）。跨模块共享的可变状态仅两处：`config` 的 RwLock 与 `translate/service` 的会话表，均有明确锁序（先 config 后 sessions，禁止嵌套反向）。

**架构不变量**：
- INV-1 明文 API Key 只存在于 Rust 进程内存与 DPAPI 密文两种形态；前端仅获得 `hasKey: bool`。
- INV-2 网络出站仅存在于 `translate/sse.rs` 与连接验证函数；其余模块禁止任何网络 IO。
- INV-3 截图位图不落盘、不经 IPC 送往前端；跨模块只传 OCR 文本与内存句柄。
- INV-4 几何量：跨进程通信与截屏坐标一律物理像素（虚拟屏全局坐标或显示器局部坐标，事件字段注明）；面板持久化尺寸一律逻辑像素。
- INV-5 取词成功之前不得显示/聚焦任何 Alya 窗口（防止夺焦使模拟复制落到自己身上）。
- INV-6 所有错误必经 error.rs 分类并映射到 EX 呈现（5.8），禁止裸 `unwrap` 吞错路径进入发布构建（clippy 门禁 deny）。
- INV-7 日志不得包含原文、译文、API Key、Authorization 头、baseUrl 全量（只记 host）。
- INV-8 活动场景、监听开关等全局状态以 Rust config 为单一事实源，前端与托盘均为订阅镜像。

### 5.5 窗口清单与生命周期

| label | 用途 | 创建时机 | 生命周期 | 关键属性 |
|---|---|---|---|---|
| `translate` | 翻译面板（划词/输入/剪贴板共用形态，截图结果附加功能区） | 启动序列末尾预创建并隐藏（D5） | 常驻复用；隐藏≠销毁；应用退出随进程结束 | 无边框、透明（CSS 圆角）、阴影、skip_taskbar、topmost 于 show 置位、失焦隐藏后清除（C3 裁决）、可拖边缘调整尺寸（松手后持久化逻辑尺寸） |
| `mask-{i}` | 截图遮罩，**每显示器一窗** | 首次截图类触发时按需创建，之后池化复用；每次触发前按当前显示器拓扑校验几何，拓扑变化则销毁重建 | 完成/取消即隐藏回池 | 无边框、不透明（自绘冻结快照）、skip_taskbar、topmost、全屏覆盖所在显示器 |
| `settings` | 设置页 | 首次打开按需创建 | 常驻隐藏复用 | 有边框、可调尺寸、单例；关闭=隐藏 |
| `onboarding` | 首启引导 / FR-11.4 配置引导 | 启动时检测"无任何已存配置"则创建并显示；FR-11.4 触发时复用（guide 模式，直达隐私要点 + Key 步骤） | 完成/跳过后隐藏保留（补配复用）；已存在有效配置则永不自动弹出（FR-11.5） | 固定尺寸向导、有边框 |

无 daemon/隐藏主窗：显示器枚举用 Tauri 2 的 AppHandle 级 `available_monitors()/cursor_position()`【假设 A5】。焦点规则：引导窗可见期间压制面板显示（排队挂起，见 5.7.3）；框选期间面板强制隐藏。

### 5.6 关键数据流

#### 5.6.1 划词翻译链路（FR-3）

```
① Alt+Q → global-shortcut 回调（Rust）                    [FR-14.1]
② action_router.selection_translate：
   a. 前置检查：onboarding 可见？→ 挂起队列，结束           [FR-11]
   b. 取词（selection 模块，全程不碰 Alya 窗口，INV-5）：
      备份当前剪贴板(内容+序号) → 置抑制标志(FR-5.7) → 向前台窗口注入 Ctrl+C
      → 轮询等待剪贴板序号变化（上限 220ms）→ 读文本
      → 恢复原剪贴板内容（恢复前校验当前内容仍是取词所得，否则放弃恢复以防覆盖用户数据）
      → 刷新监听基线 → 清抑制标志                          [FR-5.7]
③ 空文本/超时 → 系统通知"未获取到选中文本"，流程终止        [FR-3.2/3.3, EX-9/10]
④ API Key 检查（config.hasKey==false）→ 弹配置引导 + 挂起本次翻译，流程终止 [FR-3.4, FR-11.4]
⑤ 计算面板矩形（鼠标物理坐标 + 偏移 + 显示器钳制，5.7.2）→ show 面板（暖窗）
   → emit panel://show {mode:selection, text}              [FR-3.1]
⑥ 前端展示原文，invoke translate {text, source:selection}
   → service 读活动场景 → prompt.rs 构造（互译=FR-2.5 模板 / 定向=FR-2.6 模板，
     {{target}} 取面板临时目标语言，否则设置默认值）          [FR-2.3/2.5/2.6/2.7]
⑦ sse.rs 发起流式请求；chunk 事件增量推送前端；终态 done/error [FR-9.6, EX-1~5/13/14]
```

步骤 ②b 与 ⑤ 串行（INV-5），⑤ 与 ⑥⑦ 中请求发起并行——面板先弹出，网络请求异步跟进，是 J1 预算成立的前提。

#### 5.6.2 截图识别 / 截图翻译链路（FR-6/7/8）

```
① Alt+S（识别）或 Alt+D（翻译）→ action_router            [FR-6.1/7.1]
② capture 会话建立：
   a. 依次抓取全部显示器快照到内存（此时 Alya 无任何可见窗口，快照干净）
   b. 建 SessionId；按当前拓扑准备/校正 mask 窗口池，各窗载入所属屏快照
   c. 显示遮罩：暗色遮罩 + 光标十字样式；焦点交给光标所在屏的 mask 窗   [FR-8.1]
③ 用户交互（mask 页面前端）：
   - 拖拽画高亮选区（选区内透亮=快照原像素，选区外暗色）    [FR-8.1]
   - Esc → invoke cancel_selection → 隐藏遮罩，销毁会话，无后续动作  [FR-8.3]
   - 松开鼠标：位移 <4 物理 px 或面积 <64 物理像素 → 判无效，退出框选态 [FR-8.5, Q11]
   - 有效选区 → invoke complete_selection {sessionId, monitorIndex, rect(该屏局部物理px)}
④ 后端：从会话快照按 rect 裁剪 → PNG 编码入内存 → 隐藏遮罩窗 → 释放各屏快照（仅留裁剪块）
                                                          [FR-8.2, FR-8.6, INV-3]
⑤ OCR（5.7.6）：语种=设置值（auto=TryCreateFromUserProfileLanguages / 固定=指定 Language）
   → 引擎创建失败 → EX-6 通知（缺语言包 + 微软指引链接），流程终止（截图翻译同样不发翻译请求）
                                                          [EX-6, FR-6.4]
⑥ 识别空文本 → 面板提示"未识别到文字"，不发起翻译          [FR-7.2, EX-9]
⑦ 识别模式（Alt+S）：面板识别形态展示纯文本（可编辑/复制/语种下拉/一键送翻译）
   - 更换语种 → invoke reocr {sessionId, lang} 对保留的裁剪块重新识别 [FR-6.2]
   - 一键送翻译 → 走 ⑧                                    [FR-6.5]
⑧ 翻译模式（Alt+S 的送翻 / Alt+D）：API Key 检查（FR-7.3：OCR 已照常完成）
   → 按活动场景送 service 流式翻译，面板同时展示原文与译文   [FR-7.1, FR-3.4]
```

#### 5.6.3 剪贴板监听状态机（FR-5）

机制选型（风险清单第 6 条）：**事件回调（AddClipboardFormatListener）需要在 Rust 维护隐藏消息泵窗口，复杂度高；纯内容轮询（pot 方案）每 500ms 全量读剪贴板偏重。采纳折中：轮询 `GetClipboardSequenceNumber()`（user32 轻量调用）每 500ms，仅当序号变化才读一次内容。** 无消息窗口、CPU 近零、行为可测。【假设 A3】

状态机（共享于 `clipboard_monitor` 模块，一把 Mutex 保护全部字段）：

```
状态：Off / On{ baseline:(seq, text), suppressing:bool }
enable：读当前 (seq,text) 作为 baseline（开启瞬间不触发存量内容）→ On   [FR-5.2/5.3]
disable：→ Off（立即停止响应）                                        [FR-5.6]
每个 tick（500ms，仅 On 态）：
  cur_seq = GetClipboardSequenceNumber()
  cur_seq == baseline.seq → 无事
  否则读文本 read_text():
    None（图片/文件等非文本）→ baseline.seq = cur_seq；静默            [FR-5.4, EX-8]
    Some(t):
      suppressing == true → baseline = (cur_seq, t)；静默（吞掉取词写入与恢复）[FR-5.7]
      t == baseline.text → baseline.seq = cur_seq；静默（内容去重，D2）  [FR-5.5]
      t.len() > MAX_AUTO_CHARS(50k, D3) → baseline 更新；通知"内容过长已跳过"
      其余 → baseline = (cur_seq, t)；触发翻译（等同划词链路 ④~⑦，
             面板隐藏则弹出）                                          [FR-5.2]
```

FR-5.7 抑制的时序保证：取词模块在**备份之前**加同一把锁置 `suppressing=true`，恢复并刷新基线后才清标志——写入与恢复两次序号变化都落在抑制窗口内，不存在漏窗。竞态防护：恢复前读当前内容比对，若 ≠ 取词所得（期间用户自己复制了新内容）则放弃恢复，避免用旧内容覆盖用户数据；此情况下监听基线以用户新内容为准，正常触发。

开关三方同步：面板开关 / 托盘勾选 / 设置页任一处变更 → 写 config → `settings://changed` 广播 → 三处 UI 镜像更新（INV-8）。

#### 5.6.4 输入翻译链路（FR-4）

```
① Alt+E（或托盘左键/托盘菜单"输入翻译"/二次启动唤起）→ action_router.input_translate [FR-4.1, FR-12.2, FR-1.4]
② 面板定位显示（同 5.6.1 ⑤），emit panel://show {mode:input}
③ 前端聚焦原文输入区（窗口获得焦点的保证见 D7 握手与 5.7.3）
④ 空白回车 → 前端拦截，不 invoke，无错误                       [FR-4.3, EX-9]
⑤ 回车/点翻译 → invoke translate {text, source:input} → 5.6.1 ⑥⑦ 同径
⑥ 译文产出后原文可编辑，再提交生成新 roundId，译文区被新一轮替换 [FR-4.2, FR-9.6]
```

### 5.7 核心机制设计

#### 5.7.1 热键管理服务（FR-14，EX-7）

数据流：config.hotkeys（五项，出厂值见 FR-14.1）→ 启动序列逐一 `register(fn, combo)` → 每项结果收集为 `{function, combo, ok|err}` → 有失败项则聚合为**一则**系统通知列出功能名（EX-7），失败的入口进入降级态（划词不可用直至改键；其余四项托盘可用，FR-14.3）。

改键流程（设置页录制式录入）：前端校验组合合法性（至少一个修饰键 + 主键，排除纯 Shift 等）→ invoke `rebind_hotkey {function, new}` → 后端：与本服务其余四项查重（内部冲突拒绝）→ unregister 旧 → register 新 → 成功才写 config；register 失败则回退注册旧键、**不改配置值**，返回错误由设置页标红 + 系统通知（EX-7）。状态变化：任何成功的注册表变更都更新服务内 `registered: HashMap<Function, Combo>`，托盘菜单据此渲染"划词翻译热键失效"提示项（可选增强，不入验收）。

已知系统级限制（不可编码消除，列入真机矩阵）：管理员提权窗口持焦点时 UIPI 拦截低权限进程的热键【A1】；全屏独占应用与 IME 场景的 Alt 组合行为【A1】。兜底：除划词外均可经托盘触发。

#### 5.7.2 窗口定位与 DPI 策略（FR-3.1/FR-9，风险 5）

统一算法（`windows/translate.rs::place_at_cursor`）：
1. `cursor_position()` 得物理坐标（失败回落主屏中心）。
2. 遍历 `available_monitors()` 找出包含该点的显示器（含负坐标副屏；找不到回落主屏）。
3. 面板尺寸 = 持久化的逻辑宽高 × 该屏 `scale_factor()` → 物理宽高（INV-4 保证 100%/150% 缩放下视觉一致）。
4. 期望位置 = 鼠标点 + 偏移量（偏移方向与数值由 UI/UX 规范定义，缺省 +16,+16 物理px；FR-3.1"不遮挡所选区域"的精确策略属 UI 规范范畴，架构提供钳制保证）。
5. 钳制：右溢出则左移至贴屏右缘减边距，仍溢出（屏小于面板）则贴屏左缘；下溢同理；最终矩形绝不跨显示器、绝不越出所在显示器边界。

遮罩窗几何：position=显示器 origin（物理），size=显示器物理尺寸；选区矩形用该屏局部物理坐标，全局坐标 = origin + 局部。混合 DPI 下各窗独立按本屏 dpr 渲染（每窗一个 HWND，天然 per-monitor DPI aware）【A5】。

#### 5.7.3 失焦隐藏策略（FR-9.2，风险 4）

实现：Rust 侧监听 translate 窗 `WindowEvent::Focused(false)` → 若 `settings.panel.hideOnBlur==true` 且当前无压制条件 → `hide()`。压制条件（不隐藏）：

| 场景 | 规则 |
|---|---|
| 用户正在拖拽调尺寸 | 拖拽期间不失焦，无冲突；忽略 |
| 框选进行中 | 面板已在触发截图时强制隐藏，无交汇 |
| 引导窗（onboarding）可见 | 面板整体被压制不显示（挂起队列），无交汇 |
| 托盘点击 | 点击致面板失焦→隐藏，随后动作按需重开输入模式；净效果正确，瞬时闪隐可接受 |
| 从面板错误态点"打开设置" | hideOnBlur 开时面板随之隐藏（符合 FR-9.2 字面）；关时两窗并存 |
| hideOnBlur=off | 仅用户点面板可见关闭控件或 Esc 才隐藏（FR-9.2） |

失焦隐藏**不取消**进行中翻译（D10）。隐藏动作幂等（重复 hide 无副作用）；显示路径统一走 place_at_cursor + show + set_focus，避免散落的 show 调用造成状态漂移。

#### 5.7.4 截图遮罩实现（FR-8，风险 5）

方案取舍（风险清单第 5 条要求显式对比）：

| 方案 | 问题 | 结论 |
|---|---|---|
| 单一跨屏大窗（覆盖虚拟桌面包围盒） | 一个 HWND 只有一个 DPI 上下文，混 DPI 下另一屏渲染比例必然出错；包围盒含不可用区域；跨屏拖拽语义与 FR-8.4"独立框选"不符 | 否决 |
| **每显示器一窗**（各屏独立 HWND、独立缩放上下文、快照各自贴底） | 多窗管理（拓扑校验、焦点归属）；跨屏拖拽不支持——但 FR-8.4 明确"任意一个显示器上独立完成框选"，语义吻合 | **采纳** |

窗内绘制：页面 `<canvas>` 绘制冻结快照为底（D4），其上叠加 rgba(0,0,0,α≈0.35) 暗色层，拖拽时对选区矩形做挖空（clearRect/合成）实现"选区内透亮"。层级：topmost 使遮罩覆盖任务栏；因底图即含任务栏像素，视觉无缝；框选期间任务栏不可点击，属模态交互预期，Esc 退出即恢复。快照抓取发生在遮罩显示**之前**，遮罩自身永不被截入。

#### 5.7.5 裁剪与内存传递（FR-8.6）

capture 会话数据形态：`Vec<{monitor_index, RgbaImage}>`（各屏物理分辨率帧）→ 裁剪 `image::imageops::crop_imm(rect)` → `write_to(png)` 得内存字节 → 存入会话 `{id, crop_png, crop_w, crop_h}` → 丢弃各屏帧。**全程无临时文件**（与 pot 落盘缓存文件的做法刻意不同，FR-8.6 为硬约束）。裁剪块存活期：至面板关闭 / 下一次截图触发；re-OCR（FR-6.2）直接消费该内存块。OCR 输入不走文件路径：PNG 字节装入 `InMemoryRandomAccessStream` → `BitmapDecoder`（5.7.6），消除磁盘痕迹。

#### 5.7.6 OCR 封装（FR-6，风险 10）

接口：`recognize(png_bytes, lang: Auto|Tag) -> Result<Text, AppError>`。数据流：
1. `InMemoryRandomAccessStream` ← png_bytes；`BitmapDecoder` → `GetSoftwareBitmapAsync` → SoftwareBitmap（解码阻塞段放入 `spawn_blocking`，避免卡运行时）。
2. 引擎创建：`Auto` → `OcrEngine::TryCreateFromUserProfileLanguages()`（即"跟随系统首选语言"的系统口径：按用户配置语言偏好序取第一个可用项）【A6】；`Tag` → `Language::CreateLanguage` + `TryCreateFromLanguage`。
3. `RecognizeAsync().get()` → Text。
4. 错误分类：引擎创建失败/返回 null → EX-6（区分"自动模式无可用语言包"与"固定语种未安装"，附微软官方指引链接）；识别成功但空文本 → 上游按 FR-7.2 分支。
辅助：`AvailableRecognizerLanguages()` 枚举已装语言包 → 设置页"OCR 语种"下拉展示已装清单（固定语种只能从中选择，从源头减少 EX-6；枚举失败则退化为自由文本输入并在运行期报 EX-6）。设置里存的语种在使用时二次校验（语言包可能事后被卸载）。

#### 5.7.7 SSE 流式客户端与取消链路（FR-9.6/FR-10，EX-4/5/13/14，风险 9）

请求形态：`POST {base}/chat/completions`，头 `Authorization: Bearer <key>`、`Content-Type: application/json`；体 `{model, stream:true, messages:[{role:"user", content:<整模板占位符字面替换后的文本>}]}`。不附加 system 消息、不加温度等参数——保证业务基线提示词逐字生效、行为可预测。

会话模型（service.rs）：`sessions: RwLock<HashMap<Source, TranslationRound>>`，`TranslationRound{id, text, join_handle, abort: CancellationToken}`；全局 `generation: AtomicU64`。**新翻译顶替（EX-13）**：同一来源新请求 → bump generation → 旧行 `cancel()`（CancellationToken 触发 + reqwest 流被 drop → TCP 连接关闭，服务端感知断连停止生成，不残留消耗配额的幽灵连接【A7】）→ 等待旧任务 join（上限 100ms）→ 才发起新请求。保证任意时刻每来源至多一条活跃连接。

超时（D1 两级，均 tokio::time::timeout 实现）：连接至响应头 30s → 超 EX-4；流中相邻 chunk 间隔 30s → 超 EX-5（保留已收部分，标记中断，可重试）。

解析：`bytes_stream` → `eventsource_stream` → 每个 `data:` JSON 取 `choices[0].delta.content` 增量；`[DONE]` 终止；HTTP 非 2xx 按 401/403→EX-2、402→EX-3、5xx/其他→EX-14；流体内出现 error 对象 → EX-14。重试（EX-1/4/5/14 的面板按钮）= 以保留的原文与新 roundId 重新走一遍 translate（使用当前活动场景，FR-2.4"后续翻译立即生效"口径），无自动重试循环。

推送与渲染约束（FR-10.4，J2）：每个增量即时经事件推给面板；速率超约 60 事件/秒时后端合并 16ms 微批再推，保护 UI 线程。前端把 delta 直接追加到译文文本节点（D9 纯文本），不重排原文区；滚动锚定：距底 <24px 时跟随到底，用户上翻则停锚并显示"回到底部"浮标。整段模式（FR-10.3）：同一流照常消费但不渲染，等待动效 + 每 500ms 按近 2 秒字符速率外推剩余时间，界面固定冠"预估"。

#### 5.7.8 DPAPI 加密模块（风险 7）

`crypto.rs`：`protect(plain) -> cipher` = `CryptProtectData(NULL, entropy=应用常量, CRYPTPROTECT_UI_FORBIDDEN)`；`unprotect` 反向。作用域：**CurrentUser（缺省即用户域）**——密文绑定 Windows 用户账户，同机他用户不可解；Machine 作用域反而扩大暴露面，否决。熵常量仅提高跨应用拷贝门槛（同用户恶意程序本就能解 DPAPI 用户态密文，超出 v1 威胁模型，如实记录）。存储：base64 密文存 config.json 的 `ai.apiKeyCipher` 字段（6.3）；明文不落任何盘、不进日志（INV-1/7）、不出现在任何示例与工件。

解密失败降级（密码重置、配置跨用户拷贝等）：`unprotect` Err → 视同"未配置 Key"（`hasKey=false`），设置页显示未配置态，下次翻译入口走 FR-11.4 配置引导；**不崩溃、不弹系统级错误**，日志记事件（无任何值）。用户重新保存 Key 即覆写密文自愈。

#### 5.7.9 单实例 / 托盘 / 自启（FR-1.4/1.5/FR-12）

单实例：tauri-plugin-single-instance（命名互斥体）。回调（主实例侧）：解析触发来源 → `input_translate`（Q8：托盘左键与二次启动同为呼出输入翻译面板）。前台握手（D7）：新实例启动早期读 `instance.pid` → 校验进程存活 → `AllowSetForegroundWindow(pid)` → 插件检测到已有实例后退场；主实例随后 show+focus 面板即可获得前台【A8；被系统拒绝时降级为面板置前但无焦点 + 通知，记日志】。`instance.pid` 由主实例启动时写入缓存目录，退出清理。

托盘：图标 + 右键菜单七项（FR-12 表）；左键单击 → input_translate（FR-12.2）。菜单为 config 的镜像：活动场景勾选项与剪贴板监听勾选项在 `settings://changed` 时重建菜单。划词翻译不入托盘（FR-12.1）。"退出"= 清理（注销热键、停监听、删 pid 文件）后 `app.exit(0)`（FR-1.3：面板关闭只是隐藏，进程常驻；唯一退出路径在托盘）。

自启：tauri-plugin-autostart，开关开启才写启动项、关闭即移除（FR-1.5、request 权限要求）；普通用户权限运行，不申请提权。便携版移动目录会使既有自启路径失效——已知限制，重新开关一次即修复【A10】。

### 5.8 错误处理映射表（EX-1 ~ EX-14）

总原则（引自需求第 6 章）作为门禁：一切异常在面板错误态或系统通知呈现原因与下一步动作；禁止静默失败、禁止空译文或滞留等待动效冒充成功。

| 编号 | 检测点（模块·位置） | 判定依据 | 呈现层 |
|---|---|---|---|
| EX-1 | translate/sse.rs 请求建立段 | reqwest 连接类错误（DNS/TCP/TLS） | 面板错误态 + 重试按钮 |
| EX-2 | translate/sse.rs 响应头分类 | HTTP 401/403 | 面板错误态 + "检查 API Key"（按钮→open_settings） |
| EX-3 | 同上 | HTTP 402 | 面板错误态（文案指向服务商控制台），无自动重试 |
| EX-4 | sse.rs 第一级 timeout（D1） | 连接→响应头 >30s | 终止任务；面板错误态 + 重试 |
| EX-5 | sse.rs 第二级 watchdog（D1） | 流中 chunk 间隔 >30s 或流 Err | 保留已收增量 + "传输中断"标注 + 重试 |
| EX-6 | ocr/mod.rs 引擎创建 | TryCreate* 失败/null【A6】 | 系统通知：缺语言包 + 微软官方指引链接；截图翻译就此停止不误发请求 |
| EX-7 | hotkey/mod.rs 注册结果 | register Err（占用）或改键查重拒绝 | 系统通知指明功能名；托盘兜底入口保持可用 |
| EX-8 | clipboard_monitor tick | read_text None | 静默忽略，监听不受扰 |
| EX-9 | selection 空/空白；前端输入空白拦截；OCR 空文本 | 各自判定 | 分别：通知 / 无动作 / 面板提示"未识别到文字" |
| EX-10 | selection 取词超时（220ms 无剪贴板变化）或得空白 | 轮询超时 | 系统通知"未获取到选中文本"，不弹面板 |
| EX-11 | 无需检测（系统安全桌面期间进程收不到输入属预期） | — | 无特殊处理（需求明示） |
| EX-12 | single-instance 回调 | 互斥体已存在 | 唤起既有实例呼出输入面板（D7 握手保焦点） |
| EX-13 | service.rs generation bump | 同来源新请求 | 旧任务 cancel + join，仅呈现最新一轮 |
| EX-14 | sse.rs：HTTP 5xx、body 非 JSON、流内 error 事件 | 分类器 | 面板错误态"翻译服务返回异常"+ 重试，无自动重试 |

## API / 数据 / UI 影响

### 6.1 命令契约（invoke，前端→Rust）

| 命令 | 参数 → 返回 | 说明 |
|---|---|---|
| `get_settings` | — → SettingsView | API Key 仅返回 `hasKey`（INV-1） |
| `set_settings` | patch → Result | 通用设置即时保存生效（FR-13）；热键不在此通道 |
| `rebind_hotkey` | {function, combo} → Result | 改键专用，带回退语义（5.7.1） |
| `set_clipboard_monitor` | {enabled} → Result | 三方开关统一入口 |
| `translate` | {text, source} → {roundId} | 流式结果经事件返回 |
| `cancel_translation` | {roundId} → Result | 显式取消 |
| `open_settings` | — → Result | EX-2 入口；单例前置 |
| `complete_selection` | {sessionId, monitorIndex, rect} → Result | 框选确认 |
| `cancel_selection` | {sessionId} → Result | Esc 取消 |
| `reocr` | {sessionId, lang} → {text} | FR-6.2 |
| `send_ocr_to_translate` | {sessionId} → {roundId} | FR-6.5 |
| `list_ocr_languages` | — → [{tag, label}] | 已装语言包（5.7.6） |
| `verify_connection` | {base?, model?, key?} → Result | 缺省参数=用已存值；D6 请求形态 |

### 6.2 事件契约（Rust→窗口，payload 定义集中在 events.rs）

| 事件 | payload 要点 |
|---|---|
| `panel://show` | {mode: selection/input/clipboard/ocr, text?, sessionId?} |
| `translation://begin/chunk/done` | begin: {roundId, renderMode: "stream"\|"block"}（C1：分档判定为 Rust 纯函数，阈值口径 FR-10.2/Q10）；chunk: {roundId, delta}；done: {roundId} |
| `translation://error` | {roundId, ex_code, message, retriable, partial?} |
| `ocr://result` | {sessionId, text} |
| `settings://changed` | 变更补丁（三方同步：面板/托盘/设置；INV-8） |
| `selection://closed` | {sessionId}（面板据此释放语种下拉等会话态） |

### 6.3 配置模型（FR-13 全表对齐）

存储：`%APPDATA%\com.alya.app\config.json`（Tauri app_config_dir；安装版与便携版共用，Q7）。格式：UTF-8 JSON，顶层 `schemaVersion: 1`（前向迁移桩）；写入统一 `tmp 文件 + rename` 原子替换；设置变更即时保存（FR-13）。示例为出厂默认值（不含任何真实凭据）：

```json
{
  "schemaVersion": 1,
  "ai": { "baseUrl": "https://api.deepseek.com", "model": "deepseek-chat", "apiKeyCipher": null },
  "translation": { "defaultTargetLang": "英文", "presentation": "stream_all", "longTextThresholdChars": 1000 },
  "panel": { "hideOnBlur": true, "widthLogical": 460, "heightLogical": 340 },
  "clipboardMonitor": false,
  "scene": { "active": "mutual" },
  "hotkeys": { "selection": "Alt+Q", "input": "Alt+E", "ocrRecognize": "Alt+S", "ocrTranslate": "Alt+D", "toggleScene": "Alt+W" },
  "ocr": { "language": "auto" },
  "general": { "autostart": false, "theme": "system" },
  "meta": { "onboardingCompleted": false }
}
```

持久态 / 临时态分离（FR-2.3/2.7/5.3/9.3）：

| 状态 | 归属 | 重启行为 |
|---|---|---|
| 活动场景 `scene.active` | 持久 | 保持上次（FR-2.3） |
| 目标语言默认值 `defaultTargetLang` | 持久 | 生效（FR-2.7） |
| 面板临时目标语言 | **易失（内存）** | 回落默认值（FR-2.7/B3） |
| 剪贴板监听 `clipboardMonitor` | 持久 | 保持上次（FR-5.3） |
| 面板尺寸 `widthLogical/heightLogical` | 持久 | 保持（FR-9.3，存逻辑值抗 DPI 变更） |
| FR-11.4 挂起的翻译动作 | 易失（队列容量 1） | 丢弃 |
| 截图会话（裁剪块） | 易失 | 丢弃 |

定向语言清单（Q5 首批九种）作为常量表置于 prompt/config 模块，名称文案与 UI 规范共用一份。

### 6.4 UI 影响与交接点（对 UI/UX design-spec 的接口约定）

架构留给 UI 规范填充的参数：面板弹出偏移方向与距离（5.7.2 第 4 步）、遮罩暗色 α 值与选区描边样式、错误态/等待动效/预估时间的布局、"未配置 Key"引导文案（须含隐私告知要点，FR-11.4/J3）、托盘图标资源、主题色板（亮/暗/跟随系统，FR-1.2，经 CSS 变量 + `settings://changed` 生效）。UI 规范不得引入本文件之外的新窗口、新全局热键、新网络行为——如需，回本 spec 变更。

## 安全、性能、兼容性与可观测性要求

### 7.1 凭据与隐私（G5）

INV-1/2/3/7 全文有效。补充：引导与设置页 AI 分区常驻"原文将发送至第三方云端服务"告知（FR-11.2/J3）；OCR 本机完成不上传截图，仅识别出的文字在用户主动送翻时出网（第 7 章隐私义务的实现口径即 5.6.2 数据流本身）。真实 Key 仅真机联调阶段由用户注入，任何工件/代码/日志/测试出现真实凭据即为验收失败项（K3/J4）。

### 7.2 性能预算（G7/J1，风险 9 的性能面）

划词触发 → 面板可见，本地耗时 < 500ms（不含模型响应），环节上限（暖窗前提，D5）：

| 环节 | 上限 |
|---|---|
| 热键回调分发至 handler | 20ms |
| 取词全链路（备份→注入→等待→读取→恢复；内含等待剪贴板变化 220ms 超时） | 260ms |
| 配置/场景读取 + prompt 构造（内存缓存，无 IO） | 5ms |
| 面板定位计算 + show + 首帧原文渲染（暖窗） | 150ms |
| 余量 | 65ms |
| 合计 | **500ms** |

测量：Rust 侧三处打点（hotkey_received / selection_done / panel_visible）写调试日志，QA 以录屏帧数抽测复核（J1 口径：应用预热后的常规触发）。超标处置：进入专项性能任务，不得私下放宽验收口径（需求风险表）。流式体验 J2 由 5.7.7 的推送/渲染约束承载（微批 ≤16ms、纯文本节点追加、滚动锚定）。

### 7.3 兼容性

Windows 10 x64（1903+，WebView2 Runtime 基线）与 Windows 11；混合 DPI 多显示器（I4 抽查 100%/150%）；RDP/安全桌面属已知系统限制（EX-11）。OCR 能力随系统语言包（EX-6 分支兜底）。

### 7.4 可观测性

tauri-plugin-log 文件日志（app_config_dir/logs，按天滚动，Info 级；Debug 级含性能打点，发布默认关）。内容纪律见 INV-7；错误日志携带 ex_code 便于 QA 对表 5.8 归类。无遥测上报（request 约束）。

## 风险、依赖、迁移与回滚

### 8.1 技术负责人遗留风险逐条消解

| # | 遗留风险 | 消解设计 | 残余与真机验证 |
|---|---|---|---|
| 1 | 热键注册失败可感知 / UIPI / Alt 全屏与 IME | 5.7.1：注册结果聚合通知 + 降级态 + 托盘兜底 + 改键恢复；UIPI 与全屏/IME 无法编码消除 | 【A1】真机矩阵：提权窗持焦点、独占全屏游戏、IME 激活态三场景逐一验证并记录为已知限制文档 |
| 2 | 模拟复制副作用 / 剪贴板竞争 / FR-5.7 抑制 / 误报 / 500ms 含全链路 | 5.6.1②b 时序（备份→抑制→注入→恢复前校验）+ 5.6.3 同锁抑制窗；预算 7.2 给取词 260ms 硬顶；终端类 Ctrl+C 注入风险列入真机清单 | 【A2】主流软件 × 终端类（cmd/pwsh/VSCode 终端）副作用矩阵；杀软误报观察（签名缺失期）；竞争窗口用户并发复制的放弃恢复路径演练 |
| 3 | 后台唤起夺焦（SetForegroundWindow 前台锁） | D7：新实例 AllowSetForegroundWindow 握手；托盘/热键路径自带用户手势不受限 | 【A8】干净机验证二次启动焦点落地；被拒降级路径演练 |
| 4 | 失焦隐与托盘/引导窗/框选焦点交汇 | 5.7.3 交汇矩阵逐场景规定；引导期压制面板、框选期面板强制隐藏 | 交汇回归纳入 H 组手测 |
| 5 | 混合 DPI 坐标 / 任务栏层级 | 5.7.2 物理像素约定 + 每屏一窗独立 DPI 上下文；INV-4；遮罩 topmost 贴快照无缝（5.7.4） | 【A5】100%/150% × 主副屏互换矩阵（I4/F3） |
| 6 | 剪贴板感知选型 / 去重状态机 / FR-5.7 联合 / 超大文本 | 5.6.3：序号轮询 + 内容去重（D2）+ 同锁抑制 + 50k 上限（D3，待确认项） | 【A3】RDP 剪贴板重定向场景抽查 |
| 7 | DPAPI 作用域 / 解密失败降级 | 5.7.8：CurrentUser + 常量熵；失败→视同未配置走 FR-11.4，不崩溃 | 密码重置后旧密文降级路径手测 |
| 8 | WebView2 缺失处置 | 8.4：安装版 downloadBootstrapper + 失败指引；便携版启动前注册表自检 + 原生消息框指引 | 【A4】干净 VM（无 Runtime）双发布物各验一次；无网环境安装表现 |
| 9 | 流式渲染 / 取消链路幽灵连接 | 5.7.7：generation + CancellationToken + drop 断连 + join 后再发新请求；渲染约束与微批 | 【A7】mock 服务端计数断言"旧连接已关闭"集成测试；真机长文观察 J2 |
| 10 | OcrEngine 语种口径 / 语言包枚举 | 5.7.6：auto=TryCreateFromUserProfileLanguages 即系统首选语言口径；AvailableRecognizerLanguages 供设置下拉 | 【A6】真机核对枚举内容、缺包错误形态（HRESULT/null）与识别语言归属 |

### 8.2 依赖与许可

上游依赖：用户 DeepSeek Key、DeepSeek 服务可用性、系统 OCR 组件、工具链（requirements 第 9 章已列）。新增注意：全部 crates 选用宽许可（MIT/Apache/BSD 系）；**GPL 门禁**——禁止复制 pot-desktop 任何源码；code review 阶段设专项核对（借鉴清单：selection 模拟复制时序、剪贴板序号轮询思路、每屏遮罩思路、OCR 经 windows crate 的 API 序列——均为公开机制表述，非代码）。

### 8.3 迁移与回滚

全新应用无数据迁移；`schemaVersion` 为未来迁移预留。回滚：卸载即清除程序与自启项（K1 验证），删除 `%APPDATA%\com.alya.app` 即完全重置（含密钥密文，Q7 卸载默认保留用户数据）；无常驻服务/驱动残留，无版本间接口兼容负担。

### 8.4 打包与分发（G6/K 组）

- 版本号：SemVer，自 v1.0.0 起；`Cargo.toml`、`tauri.conf.json`、产物名三处同源，构建前脚本校验一致（tasks 落实）。
- identifier：`com.alya.app`（D8，发布前冻结）。
- NSIS（x64）：`installMode: currentUser`（普通用户免提权，符合 request 权限要求）；`webviewInstallMode: downloadBootstrapper`（默认在线装 Runtime）——取舍：fixedRuntime 离线无忧但体积膨胀约 200MB，v1 不接受；安装期无网导致 Runtime 安装失败时，安装界面文案给出微软官方 WebView2 下载指引【A4】。卸载保留 `%APPDATA%` 用户数据（Q7），K1 验证托盘/进程/自启项清净。
- 便携版 zip：主 exe（资源内嵌）+ README.txt（使用说明、WebView2 缺失处置说明）。启动序列最早段（任何 WebView 创建之前）以 winreg 探测 Runtime 存在性【A4】，缺失时 `MessageBoxW`（原生，不依赖 WebView）指引用户安装后重试。
- 不含自动更新（K3）；无代码签名，SmartScreen 首次运行提示为已知限制，README 说明（不伪造任何签名材料）。

## 可追溯性对照

| 需求 | 设计位置 |
|---|---|
| FR-1.1~1.5 | 5.1、5.5、5.7.9、6.3 |
| FR-2.1~2.7 | 4（权威源声明）、5.6 各链路⑥、5.7.7 请求形态、6.3 分离表 |
| FR-3.1~3.4 | 5.6.1、5.7.1/5.7.2、5.8（EX-9/10） |
| FR-4.1~4.4 | 5.6.4 |
| FR-5.1~5.7 | 5.6.3、6.2 settings://changed、D2/D3 |
| FR-6.1~6.5 | 5.6.2、5.7.5/5.7.6、6.1 reocr/send_ocr_to_translate |
| FR-7.1~7.3 | 5.6.2⑧、5.8 EX-6/9 |
| FR-8.1~8.6 | 5.7.4/5.7.5、5.6.2、D4 |
| FR-9.1~9.6 | 5.5、5.7.2/5.7.3、D9/D10、5.7.7 |
| FR-10.1~10.4 | 6.3 schema、5.7.7 推送渲染、D1 |
| FR-11.1~11.5 | 5.5 onboarding、5.6.1②a/④、D6、6.3 meta |
| FR-12 全部 | 5.7.9 |
| FR-13 全表 | 6.3 schema 逐字段对齐 |
| FR-14.1~14.3 | 5.7.1、5.8 EX-7 |
| EX-1~EX-14 | 5.8 映射表（含检测点） |

## 验收标准（测试策略概要，供 QA 展开 checklist）

分层与范围（详细用例由 QA 依 requirements 第 8 章 A~K 组展开；此处限定工程侧准备物）：

1. **单元测试（Rust）**：config 读写往返与原子写；DPAPI 加解密往返（哑数据）；prompt 构造快照测试（FR-2.5/2.6 逐字 + {{target}}/{{text}} 替换）；剪贴板状态机全转移表（新文本/重复/非文本/抑制窗/恢复竞态/超限）；SSE 解析（分包粘包/[DONE]/流内 error/中断）；两级超时分类（EX-4 vs EX-5）；呈现模式阈值边界（999/1000/1001，含空白口径 Q10）；面板钳制算法（四角/负坐标副屏/小屏）。
2. **集成测试（mock OpenAI 兼容服务，如 wiremock）**：EX-1/2/3/4/5/14 全分支自动化；取消链路断言服务端连接数归零（风险 9）；重试不叠加请求。
3. **前端测试（vitest + testing-library）**：流式追加与滚动锚定、错误态重试控件、设置表单联动、三方开关同步镜像。
4. **真机矩阵（人工，重点覆盖假设与风险）**：A1 热键三险场景（提权焦点/独占全屏/IME）；A2 取词软件清单（记事本/浏览器/Office/VSCode/终端类/Electron 应用）× 副作用观察；A3 RDP 剪贴板；A4 干净 VM 无 Runtime 双发布物 + 无网安装；A5/I4/F3 DPI×显示器组合；A6 语言包增删前后行为；A7 长文流式中断（G5）；A8 二次启动焦点；密码重置 DPAPI 降级；UAC 安全桌面（EX-11）；杀软误报观察；便携版+安装版配置共用（Q7）与卸载残留（K1）。
5. **发布物验收**：K1~K3 + J4（无明文 Key、无翻译内容文件）逐项过。

门禁提醒：本 spec 获主 Agent APPROVED 前，上述任何一项不得提前实施；实施顺序与拆解由技术负责人 tasks.md 承接。
