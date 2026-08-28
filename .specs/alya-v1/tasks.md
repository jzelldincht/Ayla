# 实施任务

> Task ID: alya-v1
> 版本: v0.1
> 所有者: 技术负责人
> 基于: spec.md v0.3 + design-spec.md v0.2
> 状态: APPROVED
> 审核者: 主 Agent（C1~C4 裁决见 0.4.1）
> 风险等级: MEDIUM
> 最后更新: 2026-08-26

## 0. 说明

### 0.1 上游与边界

- 唯一实施依据：requirements.md v0.4、spec.md v0.3、design-spec.md v0.2（均 APPROVED）与 docs/adr/0001。超出 spec「架构级明确不做」与需求第 3 章非目标的实现即为扩大范围，须回主 Agent。
- 提示词正文以 requirements.md FR-2.5 / FR-2.6 为唯一权威源（spec §4）：实现与快照测试逐字对照该文件，代码与文档不得另存变体副本。
- 参考库 `D:\WorkSpace\PROJECT\_ai\_refs\pot-desktop` 只读借鉴机制，严禁复制代码；借鉴清单见 spec 8.2，GPL 门禁见 0.2 G4。
- 真机专项（多屏 / DPI / 热键占用 / 干净 VM / 真实 Key 联调）不进入任务链，统一由 checklist.md「真机矩阵」承接，由 QA 执行。

### 0.2 通用工程门禁（每个涉码任务验收必过，各任务 Verification 不再重复罗列）

- G1：`cargo fmt --check` 通过（Rust 任务）。
- G2：`cargo clippy --all-targets -- -D warnings` 通过；发布路径禁止裸 `unwrap` / `expect` 吞错（INV-6；lint 配置于 T1 建立）。
- G3：`pnpm lint`、`pnpm build`（含 svelte-check）通过（涉前端任务）。
- G4：GPL 门禁自查：新增/修改代码对照借鉴清单（spec 8.2）核对，无 pot-desktop 实质性同构代码。
- G5：凭据与日志纪律：无真实凭据进入代码/测试/工件/日志（K3/J4）；日志遵守 INV-7。
- G6：所属模块单元/集成测试随任务交付并通过（不设尾段测试任务，见 0.3）。

### 0.3 阶段骨架与调整说明

采纳建议骨架 P0→P5。撤销建议中的 P6「测试补全」独立尾段——依拆解要求，单元/集成测试随所属模块任务交付（写入各任务 Verification）；全量门禁收敛运行分别嵌入 T16（功能集成末端）与 T17（发布前置）。打磨项按内聚归位：定位钳制/失焦隐藏入 T11，主题入 T12/T13，单实例握手入 T15，性能打点分散于 T4/T6/T11 并在 T16 汇总实测，故 P4 仅含汇聚任务 T16 一个。

### 0.4 待澄清（实施前须主 Agent / 架构师裁决；本文档未因此改动任何上游设计）

| # | 事项 | 矛盾 / 缺口 | 建议 |
|---|---|---|---|
| C1 | 分档呈现判定归属层 | spec「测试策略概要」将“呈现模式阈值边界（999/1000/1001）”列入 Rust 单测，但 6.1/6.2 契约未把呈现模式传给前端（`translation://begin` 仅 {roundId}），前端无从得知本轮应流式还是整段 | 建议 `translation://begin` payload 增补 `renderMode` 字段（最小契约扩展，判定纯函数置于 Rust 并单测）；备选：判定移前端、vitest 覆盖（偏离 spec 测试策略表述）。请架构师裁决后 T7/T12 据此落地 |
| C2 | 选区矩形获取能力缺口 | design-spec 3.4 第 3 条“选区避让（硬约束）”前提是“系统层能取得文本选区/光标的屏幕矩形”，但 spec 5.3 依赖清单没有对应系统能力（未引入 UI Automation 等） | 建议 v1 按 design-spec 自带退化路径实施（无法取得选区矩形 → 鼠标锚点规则），不新增未经批准的依赖；若必须做选区避让，回 spec 变更补充技术方案 |
| C3 | 面板置顶语义 | spec 5.5 translate 窗关键属性含 always_on_top；design-spec 3.1 为“弹出瞬间置顶一次，不做常驻置顶” | 建议折中：show 时置位 topmost、失焦隐藏后清除，兼顾两处字面；请架构师确认该折中或指定其一 |
| C4 | 图标资产来源 | design-spec 第 10 章明示“不要求本任务绘制”图标，但托盘多尺寸 ICO 与应用图标是 T15/T17 打包必需资产 | 建议实施者先行制作符合 16px 剪影测试的临时线性风格品牌占位图标（非 pot 资产），正式图标后续由主 Agent/用户提供替换；请在批准时一并确认 |

### 0.5 角色约定

实施工程师＝Rust 密集实施角色；前端工程师＝界面实施角色；跨端任务在 Owner 中标注协作方。

### 0.4.1 主 Agent 裁决（2026-08-26，随 APPROVED 生效）

| # | 裁决 |
|---|---|
| C1 | 采纳建议：`translation://begin` 增补 `renderMode: "stream"\|"block"`；分档判定为 Rust 纯函数并单测（999/1000/1001，含空白口径）。spec.md 已升 v0.3 同步 6.2 契约；T7/T12 据此落地 |
| C2 | v1 按 design-spec 3.4 自带退化路径实施（无法取得选区矩形 → 鼠标锚点规则），不新增 UI Automation 等依赖；T11 的"选区避让"步骤即指该退化规则 |
| C3 | 采纳折中：topmost 于 show 置位、失焦隐藏后清除；spec.md 5.5 已同步措辞（v0.3）；T11/T12 按此实现 |
| C4 | 批准占位图标方案：实施者自制原创线性风格占位图标（托盘多尺寸 ICO 过 16px 剪影测试、亮暗任务栏可读），禁止取用 pot 资产；正式品牌图标后续由用户提供替换，不阻塞发布 |

## 1. 任务清单

### P0 脚手架

- [x] T1：Tauri 2 + Svelte 5 工程脚手架与窗口骨架
  - Owner: 实施工程师
  - Depends on: N/A
  - 涉及: 仓库根（pnpm workspace、package.json、Vite 配置、rustfmt/clippy 配置含 unwrap_used=deny）、`src-tauri/`（Cargo.toml、tauri.conf.json、build.rs）、`src-tauri/src/`（main.rs、lib.rs 装配骨架与插件注册顺序占位）、`src/`（translate/mask/settings/onboarding 四窗口入口占位页、`src/lib/` 共享层目录）
  - 要点: identifier=`com.alya.app`（D8，发布 1.0.0 前唯一可改窗口期）；依赖清单按 spec 5.3 锁定当时最新稳定版本记入 Cargo.toml；四窗口 label 与关键属性按 spec 5.5 配置（translate 预创建挂点到 T11）；Svelte 5 runes + TS + Vite（spec 5.2）；界面语言简体中文（FR-1.1）；不引入 tiny_http/updater/sql/store（spec 5.3 明确排除项）
  - Verification: `cargo check`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`pnpm lint`、`pnpm build` 全绿；`pnpm tauri dev` 可启动且四窗口占位页可显隐（人工冒烟）；tauri.conf.json 中 identifier=com.alya.app
  - 追溯: D8、spec 5.2/5.3/5.5、FR-1.1、INV-6（门禁配置）
  - Status: DONE（2026-08-26 主 Agent 验收：六门禁全 PASS——cargo check / clippy -D warnings / fmt --check / cargo test / pnpm lint / pnpm build；环境修复：Windows 11 SDK 10.0.26100 安装[经用户授权]、KitsRoot10 注册表指向校正、SDK Lib/Include 目录大小写敏感禁用[26100 已知问题]；tauri.conf.json 修正：移除非法 bundle 目标 "zip"、webviewInstallMode 移至 bundle.windows 本级；lib.rs 文档注释缩进修复）

### P1 配置与安全基线

- [x] T2：config 模块（Settings schema、原子持久化、迁移桩）
  - Owner: 实施工程师
  - Depends on: T1
  - 涉及: `src-tauri/src/config/mod.rs`
  - 要点: Settings 结构体逐字段对齐 spec 6.3，出厂默认值=FR-13 全表；`tmp 文件 + rename` 原子写；`schemaVersion:1` 前向迁移桩；存储 `%APPDATA%\com.alya.app\config.json`（Q7：安装/便携共用）；持久/临时态分离表落实（活动场景持久、面板临时目标语言易失、面板尺寸存逻辑像素）；Q5 九种定向语言常量表置于本模块；config RwLock 与锁序声明（先 config 后 sessions，禁止嵌套反向）
  - Verification: `cargo test config`——读写往返、出厂默认值、原子写、未知字段容忍、schemaVersion 迁移桩、语言清单常量
  - 追溯: FR-13、FR-2.3、FR-2.7、FR-5.3、FR-9.3、FR-10.2、FR-14.1、spec 6.3、Q5/Q7/Q10、INV-8
  - Status: DONE（2026-08-26 实施工程师交付：Settings 逐字段对齐 spec 6.3（camelCase），默认值严格 FR-13 全表（含面板 460×340 对齐 spec v0.2）；tmp+rename 原子写（sync_all+重试兜底）；未知字段 flatten 保留透传；schemaVersion=1 迁移桩（缺失键视 v0→升 v1 回写，未来版本容忍不重写）；Q5 九语言常量表+校验；全局 RwLock 访问器（锁序声明先 config 后 sessions）；app_config_dir 解析+测试注入临时目录；`cargo test config` 17 通过（含 crypto 4），全量 28/28，fmt/clippy -D warnings PASS，未改动 tauri.conf.json，偏离见 implementation-report §5）

- [x] T3：DPAPI 加密模块 + AppError/EX 映射 + 事件契约类型
  - Owner: 实施工程师
  - Depends on: T1
  - 涉及: `src-tauri/src/config/crypto.rs`、`src-tauri/src/error.rs`、`src-tauri/src/events.rs`、`src/lib/types.ts` 与 `src/lib/ipc.ts`（TS 镜像类型与类型化 invoke 包装）
  - 要点: protect/unprotect＝CryptProtectData（CurrentUser 作用域 + 应用常量熵 + CRYPTPROTECT_UI_FORBIDDEN，spec 5.7.8）；解密失败降级＝视同未配置（hasKey=false，不崩溃不弹系统错误，记无值日志）；base64 密文存 `ai.apiKeyCipher`；AppError 携带 ex_code，覆盖 EX-1~EX-14 分类（spec 5.8）；6.1 命令参数/返回与 6.2 事件 payload 类型唯一定义于 events.rs，前端 TS 手工镜像；前端永不接触 Key 明文（INV-1）
  - Verification: `cargo test crypto`（加解密往返、篡改密文失败、降级分支）；`cargo test error`（ex_code 对 EX-1~14 编码完备性 14 条全覆盖）；`pnpm check` 类型通过
  - 追溯: spec 5.7.8/5.8/6.1/6.2、INV-1、INV-7、requirements 第 7 章凭据纪律
  - Status: DONE（2026-08-26 实施工程师交付：crypto.rs 用 windows 0.62 CryptProtectData/CryptUnprotectData（CurrentUser 缺省作用域+常量熵+CRYPTPROTECT_UI_FORBIDDEN，LocalFree 释放），base64 密文进出，解密失败 Err→config::has_key 降级"未配置"+无值日志；error.rs AppError 覆盖 EX-1~EX-14（ex_code 逐条=表 5.8，retriable=EX-1/4/5/14）+内部 Config 变体；events.rs 集中定义 6.1 全部命令参数/返回与 6.2 全部事件 payload（C1 renderMode stream|block；SettingsView 证无 Key 明文/密文 INV-1；settings://changed 广播剥离 apiKey 纪律）；types.ts/ipc.ts 手工镜像+类型化包装；`cargo test crypto` 4 / `error` 4（EX-1~14 完备性测试）+ events 7，全量 28/28，fmt/clippy -D warnings、svelte-check 0 错 0 警（pnpm check 脚本 T1 未定义，以等价 pnpm lint 验证）、pnpm build PASS；未改 tauri.conf.json，偏离见 implementation-report §5）

### P2 核心能力

- [x] T4：热键管理服务
  - Owner: 实施工程师
  - Depends on: T2、T3
  - 涉及: `src-tauri/src/hotkey/mod.rs`
  - 要点: 启动逐一注册五项并收集 `{function, combo, ok|err}`；有失败聚合为**一则**系统通知列出功能名（EX-7）；失败项进入降级态（划词不可用直至改键；其余四项托盘兜底，FR-14.3）；`rebind_hotkey`：内部五项查重拒绝 → unregister 旧 → register 新 → 成功才写 config，失败回退注册旧键且不改配置（spec 5.7.1）；registered 表维护；打点 hotkey_received（7.2）
  - Verification: `cargo test hotkey`（内部查重、回退决策、结果聚合纯逻辑）；冒烟 `pnpm tauri dev` 日志显示五项注册结果
  - 追溯: FR-14.1~14.3、EX-7、spec 5.7.1/7.2、FR-12.1（降级口径）
  - Status: DONE（2026-08-26 实施工程师交付：hotkey/mod.rs 启动五项逐一注册＋{function,combo,ok|err} 聚合＋EX-7 一则系统通知列功能名＋失败入降级态（degraded 清单/registered_snapshot 供 T15）；rebind_hotkey 命令（已注册进 invoke_handler）＝内部查重→解旧→注新→成功才写 config，失败回退旧键不改配置，返回 AppError（EX-7）；HotkeyAction 五动作枚举＋HotkeyCallback trait 注入接口（T16 接线，未接线仅 Debug 打点）；hotkey_received 打点；`cargo test hotkey` 6 通过（查重/回退决策三态/聚合/名称映射/配置读写/EX-7），全量 45/45，fmt/clippy -D warnings PASS；冒烟 pnpm tauri dev 实跑日志：五项注册成功 5/5（Alt+Q/E/S/D/W），进程与端口已清理；偏离见 implementation-report §5）

- [x] T5：剪贴板监听状态机
  - Owner: 实施工程师
  - Depends on: T2、T3
  - 涉及: `src-tauri/src/clipboard_monitor/mod.rs`
  - 要点: 每 500ms 轮询 `GetClipboardSequenceNumber()`，仅序号变化才读内容（spec 5.6.3）；enable 读当前 (seq,text) 为 baseline（存量不触发）/disable 立即失效（FR-5.6）；内容比较去重（D2）；非文本 read_text None 静默（EX-8/FR-5.4）；超 50,000 字符跳过并系统通知（D3）；同一把 Mutex 内 `suppressing` 标志及“备份前置位 / 刷新基线后清位”接口（供 T6，FR-5.7 时序保证）；`set_clipboard_monitor` 命令 + `settings://changed` 三方同步（INV-8）
  - Verification: `cargo test clipboard_monitor`——全转移表：新文本/重复文本/非文本/抑制窗吞掉写入与恢复/恢复竞态放弃恢复/超限跳过/关闭后不触发（seq 与文本读取经 trait 注入以便测试）
  - 追溯: FR-5.1~5.7、EX-8、D2、D3、spec 5.6.3、INV-8
  - Status: DONE（2026-08-26 实施工程师交付：clipboard_monitor/mod.rs 状态机＝500ms tick 仅查 GetClipboardSequenceNumber（Cargo.toml 追加 Win32_System_DataExchange feature），序号变才 arboard 读文本；enable 基线不触发存量/disable 立即失效；D2 内容去重；EX-8 非文本静默；D3 超 50,000 字符跳过＋系统通知（MAX_AUTO_CHARS 常量可调）；同一把 Mutex 的 begin_suppression（备份前置位）/end_suppression（刷新基线后清位）供 T6；set_clipboard_monitor 命令（已注册）＝写 config＋settings://changed 广播三方同步（INV-8，开启失败回滚 config），循环 tauri async_runtime spawn，代次管理关闭即退出；触发出口经 ClipboardTranslateTrigger trait 注入（T16 接线）；`cargo test clipboard_monitor` 10 通过（全转移表：新文本/重复/非文本/抑制窗吞写与恢复/恢复竞态放弃恢复/超限跳过/关闭后不触发/开启读失败/读失败跳 tick），全量 45/45，fmt/clippy -D warnings PASS；偏离与残余竞态说明见 implementation-report §4-6）

- [x] T6：取词编排（模拟复制链路）
  - Owner: 实施工程师
  - Depends on: T5
  - 涉及: `src-tauri/src/selection/mod.rs`
  - 要点: 备份剪贴板（内容+序号）→ 置抑制标志 → 向前台窗口注入 Ctrl+C → 轮询序号变化上限 220ms → 读文本 → 恢复前校验（当前内容仍是取词所得才恢复，否则放弃以防覆盖用户数据）→ 恢复 → 刷新监听基线 → 清抑制（spec 5.6.1②b）；全程不显示/聚焦任何 Alya 窗口（INV-5）；空文本/超时分路系统通知，文案区分“未获取到选中文本”（EX-9）与“未能获取文本”（EX-10，design-spec 4.4）；打点 selection_done（7.2，全链路 260ms 硬顶）
  - Verification: `cargo test selection`——注入式剪贴板/击键抽象下覆盖：成功/220ms 超时/空白/恢复竞态/抑制窗时序（置位先于备份、清位于基线刷新后）；冒烟：记事本选中文本触发，日志给出链路耗时
  - 追溯: FR-3.1~3.3、FR-5.7、EX-9、EX-10、INV-5、spec 5.6.1/7.2
  - Status: DONE（2026-08-26 实施工程师交付：selection/mod.rs＝备份(内容+序号)→begin_suppression（备份前，T5 接口）→SendInput Ctrl+C（windows Win32_UI_Input_KeyboardAndMouse，主 Agent 指定 feature 名实际不存在已按实际更名——见报告）→轮询序号变化 220ms→读文本→恢复前校验（当前≠取词所得则放弃防覆盖）→end_suppression（先刷新基线后清位）；全程不显示/聚焦 Alya 窗口（INV-5）；EX-9「未获取到选中文本」/EX-10「未能获取文本」分路系统通知（需求 v0.4 EX-10 行文与 design-spec 4.4 不一致，按 design-spec 执行——详见报告偏离）；ClipboardOps/CopyInjector trait 注入；selection_done 打点（仅字符数不记原文 INV-7）；`cargo test selection` 10 通过（成功/超时/空白/非文本/恢复竞态/抑制时序/注入失败/恢复写失败/非文本备份清残留/备份读失败，全部在监听 On 态下验证真实抑制位序：置位先于备份、清位晚于恢复、失败路径必清位、竞态放弃恢复后基线以用户内容为准）；全量 84 单测 + 11 集成 = 95/95，fmt/clippy -D warnings PASS；记事本真机冒烟依赖 action_router 接线（T16）与真机前提，留 T16 集成阶段执行；偏离见 implementation-report T6 节）

- [x] T7：翻译会话与提示词构造
  - Owner: 实施工程师
  - Depends on: T2、T3
  - 涉及: `src-tauri/src/translate/prompt.rs`、`src-tauri/src/translate/service.rs`
  - 要点: prompt.rs 场景→模板构造：互译＝FR-2.5、定向＝FR-2.6，`{{target}}`/`{{text}}` 占位符字面替换；`{{target}}` 取面板临时目标语言否则设置默认值（FR-2.7）；快照测试逐字对照 requirements.md v0.4 原文（任何文本差异触发重新评审）；service.rs 会话表 `HashMap<Source, Round>` + 全局 generation：同源新请求 bump → cancel 旧行（CancellationToken + 流 drop 断连）→ join 旧任务（上限 100ms）后才发新请求（EX-13，任意时刻每来源至多一条活跃连接）；分档判定纯函数（999/1000/1001 边界、计数含空白换行 Q10；renderMode 传递方式待 C1 裁决后落地）
  - Verification: `cargo test translate::prompt`（两模板快照、占位符替换、含特殊字符与超长文本）；`cargo test translate::service`（同源顶替、跨源隔离、取消后 join、并发不变量、阈值边界三分例）
  - 追溯: FR-2.1~2.7、FR-9.6、FR-10.1/10.2、EX-13、Q5/Q10、spec §4 权威源声明、5.7.7 会话模型
  - Status: DONE（2026-08-26 实施工程师交付：prompt.rs＝FR-2.5/FR-2.6 模板从 requirements.md v0.4 逐字收录为常量（含防乱码规则行），`{{target}}`/`{{text}}` 字面替换（replacen 恰一次，不做转义）；定向 target＝面板临时目标语言（易失槽位 set_panel_temp_target/panel_temp_target，Q5 九语言校验）否则设置默认值（service 层读 config）；快照测试把两段模板同时硬编码于测试逐字比对（注释记录源文件路径与 v0.4）；service.rs＝sessions Mutex<HashMap<Source,Round>> + 全局 generation AtomicU64 + 每来源串行门；EX-13 顶替＝bump→cancel(CancellationToken)→join(≤100ms)→才 spawn 新值；跨源隔离；decide_render_mode/count_chars/decide_presentation 纯函数（999/1000/1001 边界、Q10 含空白换行、与 C1 renderMode 全貌）；`cargo test translate::prompt` 7 通过、`service` 8 通过（同源顶替事件序/跨源隔离/取消/join 上限/并发峰值恒 1/分档边界三分例/中文 emoji 计数/stream_all 恒流式）；偏离见 implementation-report T7 节）

- [x] T8：SSE 流式客户端、连接验证与错误分类
  - Owner: 实施工程师
  - Depends on: T7
  - 涉及: `src-tauri/src/translate/sse.rs`、`src-tauri/src/translate/verify.rs`、`src-tauri/tests/sse_integration.rs`
  - 要点: `POST {base}/chat/completions`，Bearer 头，体仅 `{model, stream:true, messages:[{role:"user",...}]}`——不加 system 消息与温度等参数（spec 5.7.7）；eventsource-stream 解析 `choices[0].delta.content` / `[DONE]` / 流内 error；HTTP 401/403→EX-2、402→EX-3、5xx/非 JSON→EX-14、连接类→EX-1；D1 两级 tokio timeout：响应头前 30s→EX-4，流中相邻 chunk 间隔 30s→EX-5（保留 partial）；速率超约 60 事件/秒合并 16ms 微批再推；取消链路＝token 触发 + reqwest 流 drop 关闭 TCP（A7 幽灵连接防护）；`verify_connection` 按 D6（先 GET {base}/models，404/405 回退 max_tokens=1 最小 chat）；INV-2：全仓网络 IO 仅 sse.rs 与本验证函数
  - Verification: `cargo test --test sse_integration`（wiremock mock OpenAI 兼容服务）：EX-1/2/3/4/5/14 全分支、分包粘包/[DONE]/流内 error、EX-5 保留 partial、取消后服务端活跃连接数归零、重试不叠加请求、verify 两形态；`cargo test sse`（解析器纯逻辑单测）
  - 追溯: spec 5.7.7、D1、D6、EX-1~EX-5、EX-14、FR-9.6、FR-11.1(c)、INV-2、风险 9/A7
  - Status: DONE（2026-08-26 实施工程师交付：sse.rs＝POST {base}/chat/completions（体仅 {model,stream:true,messages:[user]}——集成测试以 body_json 全量断言）+Bearer 头；eventsource-stream 解析 delta.content/[DONE]/流内 error/非 JSON 帧；HTTP 401/403→EX-2、402→EX-3、5xx/其他→EX-14、连接类→EX-1；D1 两级 tokio timeout（响应头前 30s→EX-4、相邻 chunk 30s→EX-5 保留 partial）；RateBatcher 1s 窗口 >60 事件/秒合并 16ms 微批；取消＝CancellationToken+流 drop 断连（A7，集成测试断言服务端活跃连接归零）；verify.rs 按 D6（GET /models，404/405 回退 max_tokens=1 最小 chat，集成测试断言回退体契约）；事件经 events.rs payload emit 到 translate（SseTranslateExecutor，begin 带 C1 renderMode）；`cargo test sse` 12 通过（解析/分类/分包粘包/DONE/空 delta/流内 error/EX-5 partial/EOF 无 DONE 即中断/取消/微批三态/定时冲刷）+ verify 2 通过；`cargo test --test sse_integration` 11 通过（EX-1~EX-5/EX-14 全分支、请求形态全量契约、取消收敛、重试恰一条、verify 两形态——wiremock 0.6.5 无 set_body_stream，EX-5 停顿与取消计量用原始 TCP 夹具补足）；全量 95/95，fmt/clippy -D warnings PASS；偏离与残余风险见 implementation-report T8 节）

### P3 窗口与 UI

- [x] T9：截图会话与遮罩窗池（capture + mask）（DONE，2026-08-27 主 Agent 验收：108/108 测试全绿、clippy/fmt/lint/build 全通过；详见 implementation-report 验收附录）
  - Owner: 实施工程师（遮罩页面前端：前端工程师协作）
  - Depends on: T1、T3
  - 涉及: `src-tauri/src/capture/mod.rs`、`src-tauri/src/windows/mask.rs`、`src/mask/`（遮罩页）
  - 要点: 触发瞬间依次抓全部显示器快照入内存（D4 定格；抓取先于遮罩显示，遮罩永不被截入）；每显示器一窗、池化复用、每次触发前按拓扑校验几何否则销毁重建（spec 5.7.4）；遮罩 rgba(15,18,24,0.35) + 选区挖空透亮 + 2px accent 外框 + 1px 白色内衬描边 + 拖拽中物理像素尺寸标签 + 十字光标（design-spec 5）；松开即确认 `complete_selection{sessionId, monitorIndex, rect 该屏局部物理px}`；Esc→`cancel_selection` 无任何残留（FR-8.3/8.6）；无效判定：位移 <4 物理px 或面积 <64 物理px² 视同取消（Q11/FR-8.5）；裁剪 `crop_imm`→PNG 内存字节存会话、释放整屏帧，全程零落盘、位图不过 IPC（INV-3、5.7.5）
  - Verification: `cargo test capture`（rect 局部↔全局换算 INV-4、无效判定阈值边界、裁剪尺寸正确性）；`pnpm build`；冒烟：本机单屏走通框选→Esc/无效单击路径（多屏与 DPI 项归 checklist 真机矩阵 F 组）
  - 追溯: FR-8.1~8.6、D4、Q11、spec 5.7.4/5.7.5/5.6.2、INV-3、INV-4
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

- [x] T10：OCR 封装（Windows.Media.Ocr）（DONE，2026-08-27 主 Agent 验收：同上，EX-6 两形态与语言枚举实现确认）
  - Owner: 实施工程师
  - Depends on: T9、T3
  - 涉及: `src-tauri/src/ocr/mod.rs`
  - 要点: `recognize(png_bytes, Auto|Tag)`：PNG 字节→InMemoryRandomAccessStream→BitmapDecoder→SoftwareBitmap（阻塞解码入 spawn_blocking）；Auto=`TryCreateFromUserProfileLanguages`（系统首选语言口径 A6）、Tag=`CreateLanguage`+`TryCreateFromLanguage`；`AvailableRecognizerLanguages()` 枚举已装语言包供设置下拉（固定语种只能从中选，源头减少 EX-6）；引擎创建失败/null→EX-6（区分“自动无可用包”与“固定语种未装”，附微软官方指引链接），截图翻译就此停止不发翻译请求；使用时对所存语种二次校验（可能事后被卸载）
  - Verification: `cargo test ocr`（错误分类映射纯逻辑：HRESULT/null→EX-6 两形态、空文本交上游 FR-7.2 分支）；冒烟：对本机含文字的内存裁剪块识别出文本（不落盘）
  - 追溯: FR-6.1~6.4、FR-7.1/FR-7.2 前置、EX-6、假设 A6、spec 5.7.5/5.7.6
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

- [x] T11：翻译面板窗口生命周期与定位（DONE，2026-08-27 主 Agent 验收：132 Rust 测试全绿、clippy/fmt 通过；钳制算法含四角翻转/负坐标副屏/150%DPI/屏小于面板单测；失焦隐藏+D10+C3 topmost 逐条核对）
  - Owner: 实施工程师
  - Depends on: T2、T3
  - 涉及: `src-tauri/src/windows/translate.rs`
  - 要点: 启动序列末尾预创建并隐藏（D5 暖窗，J1 前提）；`place_at_cursor` 统一显示路径：cursor_position 物理坐标（失败回落主屏中心）→ 归属显示器（含负坐标副屏）→ 持久逻辑尺寸 × scale_factor → 偏移（+16,+16 逻辑 px，design-spec 3.4）→ 选区避让按 C2 裁决（缺省走鼠标锚点退化路径）→ 右溢出左翻 / 下溢出上翻 / 角部双翻 → 工作区内四周 8px 边距钳制，绝不跨屏（spec 5.7.2 + design-spec 3.4）；`Focused(false)` 且 hideOnBlur 开且无压制条件→hide，隐藏不取消进行中翻译（D10）；四边四角 6px 拖拽调尺寸、松手持久化逻辑宽高（FR-9.3，四形态共用单一记忆值 A4）；`panel://show` 统一发射；打点 panel_visible（7.2）
  - Verification: `cargo test windows::translate`——钳制算法：四角溢出翻转、负坐标副屏、屏小于面板、100%/150% scale 换算（spec 测试策略点名项）；冒烟：鼠标在不同屏位触发，弹出位置合理不出屏
  - 追溯: FR-3.1、FR-9.1~FR-9.3、D5、D10、spec 5.7.2/5.7.3、design-spec 3.4、INV-4、7.2、假设 A4
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

- [x] T12：翻译面板前端（四形态与状态设计）（DONE，2026-08-27 主 Agent 验收：vitest 57/57、svelte-check 0 错 0 警、pnpm build 通过；主 Agent 修复 8 项 svelte-check 错误与 StreamOutput 非响应式告警）
  - Owner: 前端工程师
  - Depends on: T3、T11
  - 涉及: `src/translate/`（面板页）、`src/lib/theme.css`（Design Tokens）、`src/lib/stores/`
  - 要点: Design Tokens 亮/暗双套 CSS 变量 + 字体栈 + 间距/圆角/阴影/动效时长（design-spec 2，±5% 明度微调不得破 4.5:1 对比度底线）；顶栏骨架：监听开关（左一，图标式）→ 互译/定向分段控件 → （定向时）语言下拉 → 幽灵关闭按钮（3.2）；形态 A/B/C/D 布局与独有控件（3.3；用户裁决 A1：所有原文区可编辑，修改后浮现「重译」，Enter 同效）；键盘契约 3.5（Esc 隐藏、Enter 提交/重译、Shift+Enter 换行、Tab 序）；状态设计 4 章：空态、呼吸点+2px 不确定进度条、流式增量淡入（≤90ms）+光标条+滚动锚定（距底 <24px 跟随、上滚停锚）、整段等待徽标「预估剩余约 N 秒」每秒刷新且“预估”醒目（FR-10.3/Q3）、六类错误卡片文案与按钮逐条照 4.4、传输中断部分结果保留（4.5）；译文纯文本节点增量追加不整绘（D9）；主题经 `settings://changed` 与系统偏好联动（FR-1.2）；减弱动效与无障碍（2px accent 焦点环、可访问名称、对比度，design-spec 9）；renderMode 消费方式待 C1 裁决
  - Verification: `pnpm vitest run src/translate`——流式追加不整段重绘、滚动锚定/停锚恢复、六类错误卡片渲染与重试/打开设置回调、重译按钮浮现逻辑、面板侧监听开关镜像；`pnpm build`
  - 追溯: FR-1.2、FR-2.4/FR-2.7（面板侧）、FR-4.1~4.3、FR-5.1、FR-9.4~9.6、FR-10.1~10.4、design-spec 2/3/4/9、D9、4.4 六类错误
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

- [x] T13：设置页（DONE，2026-08-27 实施工程师交付：见 implementation-report T13/T14 节）
  - Owner: 前端工程师（`windows/settings.rs` 窗壳：实施工程师协作）
  - Depends on: T2、T3、T4、T10
  - 涉及: `src/settings/`、`src-tauri/src/windows/settings.rs`
  - 要点: 左侧分组导航 + 右侧内容，六分组严格对齐 FR-13 十六项与出厂默认（design-spec 7.1，不另立默认值）；全页无保存按钮，任一变更即刻 `set_settings` 生效，行尾 success「已保存」1.5s 淡出（7.4）；快捷键录制控件状态机（7.2）：展示→录制（Esc/点外取消不改值）→ 解旧注新 → 成功更新键帽 / 冲突自动回退 + 行内 error + 系统通知指明功能名（EX-7）；启动即存在的冲突行尾 warning 图标常驻；API Key 固定 8 位掩码、任何视图不明文回显，「修改」流清空录入 + 录入期可见性切换 + 允许保存空值（7.3、INV-1）；OCR 语种下拉＝`list_ocr_languages` 已装清单（auto 默认）；AI 服务分区常驻“原文发送至第三方云端”隐私提示块（J3/第 7 章）；主题变更即时全窗预览；窗体默认 760×540、最小 680×480
  - Verification: `pnpm vitest run src/settings`——表单联动、即时保存 patch、录制状态机（成功/冲突回退/取消三分支）、掩码修改流、隐私块存在；冒烟：切主题、改键冲突回退、存哑值 Key 全流程
  - 追溯: FR-13 全表、FR-14.2、FR-1.2、FR-1.5、FR-6.4（枚举源头防 EX-6）、requirements 第 7 章、design-spec 7、EX-7、INV-1
  - Status: DONE（2026-08-27 实施工程师交付：settings.rs 窗壳（760×540/最小 680×480/单例隐藏复用/关闭=隐藏）+ get_settings/set_settings/open_settings 命令（set_settings＝应用补丁→原子落盘→settings://changed 广播剥离 apiKey INV-1；热键/剪贴板监听补丁拒绝走专通道；apiKey 空值=清密文）；src/settings/ 六分组十六项全量渲染自 get_settings 镜像（九语种/呈现两档/阈值/失焦即隐/剪贴板监听/五项键帽/OCR 语种/自启/主题），即时保存「已保存」1.5s 淡出+失败行内 error，HotkeyRecorder 录制控件（Esc/点外取消、修饰键+主键 e.code 组合、失败回退+design-spec 固定文案）与启动冲突 warning 图标（hotkey_registration_failures 新命令），API Key 8 位掩码修改流（清空录入/可见性切换/空值=未配置），OCR 下拉（list_ocr_languages+已存未装附注），主题三段即时预览，J3 隐私提示块；vitest 17 用例全过；偏离（autostart 仅落 config 归 T15/rebind 无广播以权威刷新补偿/文本输入 change 提交）见报告 §5；门禁全部 PASS）

- [x] T14：首次运行引导与配置引导复用（DONE，2026-08-27 实施工程师交付：见 implementation-report T13/T14 节）
  - Owner: 前端工程师（`windows/onboarding.rs` 窗壳与 meta 写入：实施工程师协作）
  - Depends on: T2、T3、T8
  - 涉及: `src/onboarding/`、`src-tauri/src/windows/onboarding.rs`
  - 要点: 四步向导 560×430（design-spec 6）：欢迎（快捷键速览）→ 隐私告知（完整句子 + 强调底色块平铺、无折叠无内部滚动、“我已了解，继续”唯一前进入口、向导不支持 Esc，FR-11.2）→ 配置 API Key（`verify_connection` 行内成功/失败反馈、可重试可跳过、录入期可见性切换）→ 完成（进入托盘常驻）；「暂不使用」＝跳过整个引导（FR-11.3 路径，截图识别完全可用）；guide 模式复用窗体承接 FR-11.4 补配（直达隐私要点 + Key 输入，补配后挂起翻译自动继续，与 T16 对接）；完成/跳过写 `meta.onboardingCompleted`；已存在有效配置永不自动弹出（FR-11.5）
  - Verification: `pnpm vitest run src/onboarding`——步骤流转、隐私步阻断、跳过路径、验证成功/失败行内反馈、guide 模式直达；冒烟：清空配置目录后首启完整走向导，二次启动不再弹出
  - 追溯: FR-11.1~11.5、D6（消费 T8 命令）、J3、design-spec 6
  - Status: DONE（2026-08-27 实施工程师交付：onboarding.rs 窗壳（固定 560×430/有边框/居中/完成跳过隐藏保留/模式切换销毁重建/CloseRequested=隐藏）+ should_show_first_run（!onboardingCompleted && !hasKey，含 DPAPI 降级口径）+ show_first_run/open_config_guide/finish_onboarding（写 meta + 广播 + 隐藏 + guide 回调 GuideFinishedHandler(hasKey) 供 T16 接线）+ onboarding://begin 复位事件（复用窗残留规避）；verify_connection 经 rename 注册为命令（T8 内部函数保留）；src/onboarding/ 四步向导（欢迎/隐私告知完整句子+强调块唯一前进/Key 配置验证连接行内反馈可重试可跳过/完成「进入 Alya」）+ guide 模式两步直达 + 不支持 Esc；vitest 11 用例全过；门禁全部 PASS（Rust 141/141、前端 85/85））

- [x] T15：托盘、单实例与开机自启（DONE，2026-08-27 主 Agent 验收：tray+instance+autostart+action_router 共 982 行，Rust 149/149 全绿；七项原生菜单/左键=输入翻译/镜像重建/D7 握手+降级/pid 写清/HKCU 自启 逐条核对）
  - Owner: 实施工程师
  - Depends on: T1、T2、T3、T4
  - 涉及: `src-tauri/src/tray.rs`、`src-tauri/src/lib.rs`（single-instance / autostart 装配）、`instance.pid` 缓存文件逻辑
  - 要点: 托盘右键七项原生菜单（当前场景单选子菜单 / 输入翻译 / 截图识别 / 截图翻译 / 剪贴板监听勾选 / 设置 / 退出；无划词项 FR-12.1；文案照 design-spec 8）；左键单击＝呼出输入翻译面板（Q8/FR-12.2）；`settings://changed` 时重建菜单镜像 config（场景勾选与监听勾选实时反映，INV-8）；single-instance 回调→`input_translate`（EX-12）；D7 前台握手：新实例早期读 `instance.pid`→校验存活→`AllowSetForegroundWindow`→退场；被系统拒绝降级为面板置前无焦点 + 通知并记日志；pid 主实例启动写入缓存目录、退出清理；autostart 开关开启才写 HKCU Run、关闭即移除（FR-1.5）；「退出」＝注销热键→停监听→删 pid→`app.exit(0)`（FR-1.3 唯一退出路径）
  - Verification: `cargo test`（pid 写读清、菜单镜像重建决策纯逻辑）；冒烟：应用运行中再次启动无第二进程且既有实例面板唤起；托盘退出后进程结束、图标消失
  - 追溯: FR-1.3~1.5、FR-2.4(c)、FR-5.1（托盘勾选）、FR-12、FR-14.3（兜底）、EX-12、D7、Q8、spec 5.7.9
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

### P4 集成打磨

- [x] T16：action_router 汇聚与全链路集成（DONE，2026-08-27 主 Agent 验收：action_router 全链路接线与挂起队列完成；Rust 146/146 单测+11/11 集成、vitest 85/85、clippy/fmt/lint/build 全通过）
  - Owner: 实施工程师
  - Depends on: T4、T5、T6、T7、T8、T9、T10、T11、T12、T13、T14、T15
  - 涉及: `src-tauri/src/action_router.rs`、各模块接线点
  - 要点: 五动作统一入口（selection_translate / input_translate / ocr_recognize / ocr_translate / toggle_scene）；依赖方向 action_router→能力模块→config/events 禁止反向，禁止绕过 action_router 直接触发翻译（保证 EX-13 顶替单路径，spec 5.4）；前置检查链：onboarding 可见→挂起队列（容量 1、易失）排队；hasKey==false→不发起网络请求、弹配置引导并挂起本次翻译，补配后自动继续（FR-11.4）；框选期面板强制隐藏、引导期压制面板（5.6.1②a、5.7.3 交汇矩阵全表落实）；EX-13 顶替、D10 失焦不取消端到端复核；监听开关三方同步端到端；7.2 三处打点贯通，Debug 日志实测“热键回调→取词完成→面板可见”分段时间（暖窗常规触发，合计目标 <500ms，超标如实上报进专项，不私下放宽口径）
  - Verification: `cargo test action_router`（路由分发、挂起队列入队/补配继续/重启丢弃、压制条件判定）；全量 `cargo test`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`pnpm vitest run`、`pnpm build` 全绿；人工走查五入口 × 互译/定向冒烟并记录；打点耗时数值写入 implementation-report（J1 工程侧证据）
  - 追溯: spec 5.4/5.6.1②a/5.7.3/7.2、FR-11.4、FR-2.4、EX-12/EX-13、D10、INV-8、5.8 全表检测点归属复核
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

### P5 打包发布物

- [x] T17：NSIS + 便携版发布物与发布门禁（DONE，2026-08-28 主 Agent 验收：版本校验通过；NSIS x64 3.43MB；便携版 zip 4.70MB 含 Alya.exe+README.txt；Rust 146/146+11/11、vitest 85/85、lint/build 全绿）
  - Owner: 实施工程师
  - Depends on: T16
  - 涉及: `src-tauri/tauri.conf.json`（bundle 配置）、`scripts/check-version.mjs`（版本一致性校验）、`src-tauri/src/portable_check.rs`（winreg 探测 WebView2 Runtime + MessageBoxW 原生指引，装配于启动最早段、任何 WebView 创建之前）、README.txt 模板
  - 要点: 版本 SemVer 自 v1.0.0 起，Cargo.toml / tauri.conf.json / 产物名三处同源，构建前校验脚本强制（spec 8.4）；NSIS x64：installMode=currentUser（免提权）、webviewInstallMode=downloadBootstrapper，安装期无网致 Runtime 安装失败的界面文案给出微软官方下载指引（A4）；卸载保留 `%APPDATA%\com.alya.app` 用户数据（Q7）；便携 zip＝主 exe + README.txt（使用说明、WebView2 缺失处置、SmartScreen 无签名首次运行提示说明、无自动更新声明）；K3 自查：无自动更新组件、全仓无真实凭据；GPL 门禁终审记录（对照 spec 8.2 借鉴清单）
  - Verification: `pnpm tauri build` 产出 NSIS exe 与便携 zip；`node scripts/check-version.mjs` 输出三处一致；本机安装/卸载一轮冒烟（托盘、进程、自启项清净）；干净 VM 与无网安装验证移交 QA（checklist K 组 / X6）
  - 追溯: G6、spec 8.4/8.2、K1~K3、Q7、A4
  - Status: DONE（已完成，详见 implementation-report.md 与 test-report.md）

## 2. 依赖总览与关键路径

- 关键路径（最长串行链）：**T1 → T3 → T7 → T8 → T14 → T16 → T17**。
- 并行支线：T2 ∥ T3；T4 / T5 / T9 在 T2、T3 就绪后可并行；T6←T5；T10←T9；T11 ∥ T9/T10；T12←T11；T13、T15 与 T11/T12 并行推进。
- 真机专项（多屏、DPI、热键占用、干净 VM、真实 Key 联调）不占任务链，统一由 checklist.md 真机矩阵承接。
