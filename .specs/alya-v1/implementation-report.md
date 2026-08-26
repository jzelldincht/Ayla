# 实施报告：T1 工程脚手架与四窗口骨架

> Task ID: alya-v1
> 版本: v0.1
> 所有者: 实施工程师
> 基于: tasks.md（APPROVED，T1 条目）+ spec.md v0.3（5.2/5.3/5.5/6.3）+ design-spec.md v0.2（0 章/3.1）
> 状态: DRAFT
> 最后更新: 2026-08-26

## 1. 概述

T1 骨架已全部创建：前端（Svelte 5 runes + TS + Vite 四 HTML 多入口 + src/lib 共享层）、
Rust 工程（src-tauri：Cargo.toml/tauri.conf.json/build.rs/main.rs/lib.rs + 12 模块占位）、
质量配置（rustfmt/clippy/ESLint/svelte-check/prettier）、原创占位图标（C4 裁决）与 git 仓库。

**关键结论：3/6 门禁通过（G1 fmt、G3 lint、G3 build），3/6 被本机环境阻塞（check/clippy/test，同一根因：
本机 Visual Studio 已装 VC 工具链但 Windows SDK（Lib）未安装，MSVC 连接器报 LNK1181 kernel32.lib 无法打开）。
阻塞项已在第 6 节给出诊断证据与升级建议，未经用户批准不擅自安装 SDK 或切换 GNU 工具链。**

## 2. 创建文件树摘要（仓库根 D:\WorkSpace\PROJECT\_ai\alya）

```
根（git 已 init 于 main，未推送）
├─ package.json                  # pnpm；scripts: dev/build/lint(eslint+svelte-check)/format/tauri
├─ pnpm-lock.yaml
├─ vite.config.ts                # 四 HTML 入口（translate/mask/settings/onboarding），dev 端口 1420，appType=mpa
├─ tsconfig.json                 # strict + bundler resolution，供 svelte-check/eslint
├─ svelte.config.js              # vitePreprocess（Svelte 5 runes）
├─ eslint.config.js              # flat config：js+ts+svelte+prettier 兼容
├─ .prettierrc.json / .prettierignore
├─ .gitignore                    # node_modules/dist/src-tauri/target|gen
├─ icons-src/                    # icon-1024.png（tauri icon 源）、icon-16/32-check.png（自检图）
├─ scripts/generate-placeholder-icon.mjs   # 零依赖 PNG 生成器（原创线性 A，可复现/可替换）
├─ src/
│  ├─ translate|mask|settings|onboarding/  # 各 index.html + main.ts + App.svelte（占位页，简体中文，显示窗口名）
│  └─ lib/
│     ├─ types.ts                # 占位（T3 落 6.1/6.2 TS 镜像）
│     ├─ ipc.ts                  # call/subscribe 类型化 invoke 包装占位
│     ├─ stores/index.ts         # 占位（INV-8 订阅镜像约定）
│     └─ theme.css               # 仅 Design Tokens 挂载点注释（T12 落地）
├─ src-tauri/
│  ├─ Cargo.toml                 # spec 5.3 全清单 + [lints.clippy] unwrap_used/expect_used=deny
│  ├─ Cargo.lock
│  ├─ .cargo/config.toml         # 本机环境修正：显式指定 MSVC link.exe（Git Bash 遮蔽问题，见第 6 节）
│  ├─ build.rs / rustfmt.toml
│  ├─ tauri.conf.json            # identifier=com.alya.app、Alya、0.1.0-dev、app.windows=[]、bundle 占位（nsis/zip）
│  ├─ capabilities/default.json  # core:default，windows=["*"]
│  ├─ icons/                     # tauri icon 生成全尺寸集（32x32/128x128/128x128@2x/icon.icns/icon.ico/icon.png + 商店图）
│  └─ src/
│     ├─ main.rs                 # windows_subsystem + alya::run()
│     ├─ lib.rs                  # 插件注册顺序（log→single-instance→global-shortcut→autostart→notification）与启动序列占位
│     ├─ config|hotkey|clipboard_monitor|selection|capture|ocr|translate|windows/     # 每 mod 一行职责注释
│     ├─ windows/{translate,mask,settings,onboarding}.rs   # spec 5.5 关键属性注释挂载点（T9/T11/T13/T14 实现）
│     └─ tray.rs|events.rs|error.rs|action_router.rs       # 占位
└─ dev-logs/2026-08-26.log        # 追加本任务事件
```

说明：`tauri.conf.json` 的 `app.windows` 为 `[]`（偏差记录见第 5 节）；四窗口 label 与关键属性
按 spec 5.5 落为 `src-tauri/src/windows/` 各文件与 mod.rs 的规范注释，由对应任务（T9/T11/T13/T14）动态创建。

## 3. 依赖及锁定版本（2026-08-26 查询最新稳定，Cargo.lock / pnpm-lock.yaml 冻结）

Rust（Cargo.toml 约束 → 锁定）：
- tauri 2.11 → 2.11.5；tauri-build 2.6 → 2.6.3
- tauri-plugin-global-shortcut 2.3.2 / single-instance 2.4.3 / autostart 2.5.1 / notification 2.3.3 / log 2.9.0
- selection 1.2.0 / screenshots 0.8.10 / image 0.25.10（default-features=false, png）/ arboard 3.6.1
- reqwest 0.12.28（default-features=false, json/stream/rustls-tls）/ eventsource-stream 0.2.3 / futures-util 0.3.34 / tokio 1.53.1
- serde 1.0.229 / serde_json 1.0.151 / thiserror 2.0.20 / winreg 0.56.0 / windows 0.62.2（spec 5.3 八个 feature）
- 说明：Cargo.lock 中出现的 image 0.24.9、thiserror 1.0.69、winreg 0.10.1/0.55.0、windows 0.51~0.61、reqwest 0.13.4 均为依赖树中间接版本。

前端（package.json ^约束 → 锁定）：@tauri-apps/api 2.11.1 · @tauri-apps/cli 2.11.4 · svelte 5.56.10 ·
vite 8.2.2 · @sveltejs/vite-plugin-svelte 7.3.0 · typescript 5.9.3 · svelte-check 4.7.6 · eslint 10.9.1 ·
eslint-plugin-svelte 3.23.0 · typescript-eslint 8.68.0 · @eslint/js 10.0.1 · globals 17.11.0 ·
eslint-config-prettier 10.1.8 · prettier 3.9.6 · prettier-plugin-svelte 4.1.1。

## 4. 六项门禁结果（全部实际运行）

| # | 门禁 | 结果 | 关键输出 |
|---|---|---|---|
| 1 | `cargo check`（src-tauri/ 下） | **FAIL（环境阻塞）** | 编译至 build 脚本链接阶段：`LNK1181: cannot open input file 'kernel32.lib'`（MSVC link.exe exit 1181）；根因：本机未装 Windows SDK（`C:\Program Files\Windows Kits\10\` 下无 `Lib\`，vswhere 查无 Windows10SDK 组件，全盘检索无 Kernel32.lib） |
| 2 | `cargo clippy --all-targets -- -D warnings` | **FAIL（同一环境阻塞）** | 与 #1 相同：编译期先在 build 脚本链接处失败，未进入 clippy 检查阶段 |
| 3 | `cargo fmt --check`（先 `cargo fmt` 全量格式化） | **PASS** | `cargo fmt && cargo fmt --check` 均无输出，返回码 0 |
| 4 | `pnpm lint`（eslint . && svelte-check） | **PASS** | `svelte-check found 0 errors and 0 warnings`；eslint 0 error（修复后复跑确认） |
| 5 | `pnpm build`（vite build 多入口） | **PASS** | 四入口全部产出：dist/src/{translate,mask,settings,onboarding}/index.html + 各自 js/css，`✓ built in 158ms`（改后复跑 214ms） |
| 6 | `cargo test` | **FAIL（同一环境阻塞）** | 无测试可执行（骨架 0 测试），但编译期同 #1 链接失败，无法运行 |

补充：`pnpm tauri dev` 按任务书要求未运行（交互式，留集成冒烟）。

## 5. 偏离 tasks.md 之处

1. **tauri.conf.json 不静态声明窗口**：spec 5.5 四窗口生命周期均为条件/按需创建（settings 首次打开、
   onboarding 无配置才弹、mask 每屏一窗、translate T11 暖窗），静态声明将导致每次启动预创建隐藏 WebView，
   违反 FR-11.5 与 D5 归属。故 `app.windows=[]`，四窗口 label 与关键属性以 `src-tauri/src/windows/*` 注释
   作为唯一在仓清单。若主 Agent 坚持静态声明，请裁决后调整。
2. **新增 `src-tauri/.cargo/config.toml`**：Git Bash（MSYS）的 `/usr/bin/link.exe`（GNU coreutils）遮蔽
   MSVC 链接器（首次 `cargo check` 报 `link: extra operand`）；显式指定 VS Community 的 link.exe 后
   暴露真正的第二层问题（缺 SDK）。路径为机器相关值，更新 VS 版本需同步（已在文件内注释）。
3. **图标生成脚本入仓**（scripts/generate-placeholder-icon.mjs）：C4 裁决「占位图标可替换」，保留
   零依赖可复现生成器便于正式图标替换时重新出全套；非 pot 资产，纯原创线性 A（#2E63DC）。
4. **cargo `-p <crate>` 单包选中会触发 cargo 1.89 resolver panic**（`did not find features for ... proc-macro2`），
   属 cargo 本仓 bug，与项目无关；全量 `cargo check` 路径不同，已避开。记录供后续任务注意。

## 6. 环境阻塞诊断与升级项（需主 Agent / 用户决策）

**现象链**：git-bash `link.exe` 遮蔽（已由 config.toml 修正）→ MSVC link.exe exit 1181 → LNK1181 kernel32.lib。

**根因**：本机存在 Visual Studio 2022 Community 的 VC 工具链（vswhere 命中
Microsoft.VisualStudio.Component.VC.Tools.x86.x64，link.exe 在
`...\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\`），但 **Windows SDK（Lib）未安装**：
- `C:\Program Files\Windows Kits\10\` 仅有 `Catalogs`、`Debuggers`，无 `Lib/`、`Include/`；
- vswhere -requires Microsoft.VisualStudio.Component.Windows10SDK 无命中；
- `find "Program Files (x86)" "Program Files" -iname Kernel32.lib`（含 SDK 常规深度）无结果；
- 注册表 `HKLM\...\Windows Kits\Installed Roots` 查询无 KitsRoot10。

**可选解**（需用户授权，均属机器级变更）：
- **A（推荐）**：Visual Studio Installer 安装「Windows 10/11 SDK」组件（或勾选「使用 C++ 的桌面开发」工作负载，
  与 Tauri 官方 MSVC 要求一致；需管理员）。安装后本机无需改任何项目文件即可还原六门禁。
- **B（不推荐）**：切换 GNU/mingw 工具链（本机 msys64 已具备完整 mingw runtime：libkernel32.a/crt2.o 在
  `/c/msys64/mingw64/lib/`，gcc 14.2.0）。Tauri 官方文档仅支持 MSVC（未提及 GNU），NSIS/WebView2 打包链路
  存在不确定性，且需改 rustup 工具链与项目配置，属架构性偏离，需主 Agent 决策。

**升级请求**：请主 Agent 将本阻塞项与 A/B 选项交用户；批准 A 后重跑 T1 三个 Rust 门禁即可（代码无需修改）；
批准 B 则先由架构师评估后再实施。未获批准前本任务状态保持 DRAFT，tasks.md 中 T1 维持原 Status。

## 7. 遗留问题

1. Rust 三门禁（check/clippy/test）待环境修复后重跑（见第 6 节）。
2. 真机冒烟（`pnpm tauri dev` 启动与四窗口占位页显隐）留至集成冒烟阶段。
3. 图标为占位（C4）：正式品牌图标由用户提供后以 `pnpm tauri icon` 重新生成即可，生成器已留档。
4. `bundle` 配置为占位（targets=nsis/zip、installMode=currentUser、downloadBootstrapper），T17 收窄细化。
5. `translate` 窗未预创建（按任务书[T11 挂点]）；如主 Agent 希望在 T1 冒烟期看到窗口，需要 T11 提前或
   给 tauri.conf.json 增窗口声明（见第 5 节第 1 条）。

---

## 主 Agent 验收附录（T1，2026-08-26）

- 结论：**APPROVED**。六项门禁在主 Agent 环境下复跑全 PASS（cargo check / clippy --all-targets -D warnings / fmt --check / cargo test / pnpm lint / pnpm build）。
- 实施者交付时 Rust 门禁 FAIL 的根因为本机缺 Windows SDK，非代码缺陷；主 Agent 获用户授权后完成以下环境修复并复验：
  1. VS Installer 静默补装 Windows 11 SDK 10.0.26100.0；
  2. 修正注册表 KitsRoot10（原指向 C:\Program Files\Windows Kits\10\，实际库位于 (x86) 树）；
  3. fsutil 禁用 SDK Lib/Include 目录的逐目录大小写敏感属性（SDK 26100 已知问题，导致 MSVC link 找不到 DbgHelp.Lib）。
- 主 Agent 对脚手架的代码修正：tauri.conf.json 移除非法 bundle 目标 "zip"（便携 zip 由 T17 脚本自产）、webviewInstallMode 从 nsis 内部移至 bundle.windows 本级；src/lib.rs 文档注释缩进修复。
- 遗留：pnpm tauri dev 冒烟留待 T16 集成阶段执行。

---

# 实施报告：T2 config 模块 + T3 DPAPI 加密 / AppError·EX 映射 / 事件契约类型

> Task ID: alya-v1
> 版本: v0.1
> 所有者: 实施工程师
> 基于: tasks.md（APPROVED，T2/T3 条目）+ spec.md v0.3（5.4/5.7.8/5.8/6.1/6.2/6.3）+ requirements.md v0.4（FR-13 全表、Q5）
> 状态: DRAFT，交主 Agent 验收
> 最后更新: 2026-08-26

## 1. 概述

T2 与 T3 一并交付（关键路径相邻且契约强耦合）：
- **T2**：`config/mod.rs` 全量实现——Settings 结构体逐字段对齐 spec 6.3 示例 JSON（camelCase 序列化），
  出厂默认值严格照 FR-13 全表（本文件为默认值唯一权威源）；`tmp + rename` 原子写；未知字段经
  `#[serde(flatten)] extra` 保留透传；`schemaVersion: 1` 迁移桩（旧版无键视为 v0 → 升 v1 回写；
  未来版本容忍不重写）；Q5 九种定向语言常量表；全局 `RwLock<Settings>` 访问器与锁序声明
  （先 config 后 sessions）；路径经 tauri `app_config_dir` 解析（测试注入临时目录，不碰真实 %APPDATA%）。
- **T3**：`config/crypto.rs`（DPAPI：CryptProtectData/CurrentUser 缺省作用域 + 应用常量熵 +
  CRYPTPROTECT_UI_FORBIDDEN，base64 密文进出，LocalFree 释放，解密失败 Err 由 `config::has_key`
  降级为「未配置」并记无值日志）；`error.rs`（AppError 携带 ex_code，EX-1~EX-14 全覆盖，含
  retriable 口径）；`events.rs`（6.1 命令参数/返回与 6.2 事件 payload 唯一定义处，含 C1 裁决的
  `renderMode: "stream"|"block"`）；`src/lib/types.ts` 手工镜像 + `src/lib/ipc.ts` 类型化 invoke 包装。

## 2. 修改文件清单

| 文件 | 状态 | 说明 |
|---|---|---|
| `src-tauri/Cargo.toml` | 修改 | 新增 `base64 0.22`、`log 0.4`（见第 5 节偏离 1）；windows features 未动（LocalFree 经核实位于 Win32_Foundation，已在清单） |
| `src-tauri/src/config/mod.rs` | 占位→实现 | Settings/schema/原子写/迁移桩/语言表/全局访问器/has_key/app_config_dir + 13 单测 |
| `src-tauri/src/config/crypto.rs` | 新建 | DPAPI protect/unprotect + CryptoError + 4 单测 |
| `src-tauri/src/error.rs` | 占位→实现 | AppError（EX-1~14 + Config 内部变体）+ 4 单测 |
| `src-tauri/src/events.rs` | 占位→实现 | 8 事件名常量、全部命令 DTO、8 事件 payload、SettingsView/SettingsPatch + 7 单测 |
| `src/lib/types.ts` | 占位→实现 | TS 镜像（与 events.rs 手工同步） |
| `src/lib/ipc.ts` | 占位→实现 | 14 命令 + 8 事件订阅的类型化包装 |
| `.specs/alya-v1/implementation-report.md` | 追加 | 本节 |
| `.specs/alya-v1/tasks.md` | 修改 | T2/T3 Status → DONE（附证据行） |
| `dev-logs/2026-08-26.log` | 追加 | 任务事件 |

未改动：`tauri.conf.json`、lib.rs（装配不动，T16 接线）、其他任务范围文件。
命令注册（`invoke_handler`/`setup`）不在 T2/T3 范围，留给 T13/T16 按 events.rs 类型装配。

## 3. 门禁结果（全部实际运行）

| # | 门禁 | 结果 | 关键输出 |
|---|---|---|---|
| G1 | `cargo fmt --check` | **PASS** | 无输出，返回码 0 |
| G2 | `cargo clippy --all-targets -- -D warnings` | **PASS** | Finished（0 warning；本任务为 `unwrap_used/expect_used=deny` 全程约束，测试亦无裸 unwrap/expect） |
| G6 | `cargo test`（全量） | **PASS** | **28 passed; 0 failed**（config 13 / crypto 4 / error 4 / events 7） |
| — | `cargo test config` / `crypto` / `error` | **PASS** | 17 / 4 / 6 通过（error 筛选含其余模块中命名带 "error" 的 2 例：events 1 + config 1，属筛选口径；全量清洁数 28） |
| G3 | `pnpm lint`（eslint + svelte-check） | **PASS** | `svelte-check found 0 errors and 0 warnings`；eslint 0 error |
| G3 | `pnpm build`（vite 四入口） | **PASS** | `✓ built in 167ms` |
| — | `pnpm exec prettier --check`（types.ts/ipc.ts） | **PASS** | 无风格问题 |
| G4 | GPL 门禁自查 | **PASS** | 纯 serde/DPAPI/契约类型实现，无任何 pot-desktop 同构代码（未引入其任何实现） |
| G5 | 凭据与日志纪律 | **PASS** | 全部测试使用哑值（sk-fake-*）；无真实凭据；日志无 Key/密文/原文值（has_key 降级日志仅记无值类别） |

关键测试覆盖点（对应任务 Verification）：
- config：读写往返、出厂默认值全表、原子写（覆盖保存无 tmp 残留）、未知字段容忍+透传回写、
  v0→v1 迁移桩持久化、未来版本容忍不重写、损坏 JSON 报错不 panic、spec 6.3 示例字段集合对齐、
  Q5 九语言文案/唯一性、语言查找校验、has_key 三分支、全局访问器（未初始化报错/重复 init 拒绝）。
- crypto：protect/unprotect 往返（哑数据）、密文篡改失败（base64 非法与内容篡改两形态）、
  异熵密文不可解（跨应用门槛）、降级分支 has_key=false。
- error：EX-1~14 ex_code 完备性 14 条、编码唯一性、retriable 口径、Display 用户侧文案。
- events：payload 线缆形状（camelCase/ex_code 下划线/缺省字段不出现）、renderMode C1 形状、
  SettingsView 无 Key 明文/密文键（INV-1）、补丁空值省略、apiKey 仅写入通道出现。

## 4. DPAPI 与密钥路径说明（INV-1 证据链）

- 明文 Key 仅在用户输入后的 Rust 内存：`SettingsPatch.ai.api_key`（IPC JSON，序列化键 `apiKey`）
  → 后端即调 `crypto::protect` → 密文 base64 存入 `ai.apiKeyCipher` → `config.json`（磁盘仅密文）。
- 前端唯一 Key 状态：`get_settings` 返回 `SettingsView.ai.hasKey`（经 `config::has_key` 实测
  DPAPI 解出为 true，否则降级 false）。
- `settings://changed` 载荷类型与 `SettingsPatch` 相同，但**广播构造必须剥离 apiKey**（仅后端发射点执行；
  events.rs 文档注释固化，T16 装配时按此实现）。

## 5. 偏离与契约填充说明（需主 Agent 知悉，无静默决策）

1. **Cargo.toml 新增两依赖**（spec 5.3 清单未列）：`base64 0.22`（T3 明确要求 base64 密文，
   清单遗漏）与 `log 0.4`（spec 5.7.8 要求的降级无值日志需要 log facade；tauri-plugin-log 即基于它）。
   windows features 无需调整（核实 windows-0.62.2 源码：`LocalFree` 在 Win32_Foundation）。
2. **AppError 额外内部变体 `Config`**（ex_code=`"config"`）：spec 5.8 未定义配置读写失败的用户呈现，
   为闭合 INV-6「一切错误经 error.rs 分类」而设；不进入 EX 呈现表（前端六类错误卡不含它），
   T16 装配时决定向上呈现方式。若主 Agent 认为应从 enum 移除改为模块内错误，请裁决。
3. **`ex_code` 保留下划线**（`#[serde(rename = "ex_code")]`）：spec 6.2 字面为 `ex_code`（与
   roundId camelCase 混用即契约原样），TS 镜像同步。前端消费时直接按 `ex_code` 访问。
4. **契约填充（spec 未细化的子键名）**：`set_settings` 补丁内 Key 明文字段序列化键为 `apiKey`
   （与存储键 `apiKeyCipher` 语义区分）；`presentation` 第二档序列化为 `"staged"`（"分档呈现"；
   spec 示例只给出 `"stream_all"`）；`translation://error` 的 retriable/partial 属性含义已在
   events.rs/error.rs 文档固化（EX-1/4/5/14 为 true）。
5. **迁移桩行为细化**：无 `schemaVersion` 键的旧文件视为 v0 → serde 按字段默认值合并缺省字段 →
   `migrate` 升 v1 → 立即回写磁盘；`schemaVersion > 1`（未来版本）容忍读取、不降级不重写。
   （实现之初版本缺省即 1 导致桩永不触发，已修正为缺省 0，见测试 `migration_stub_upgrades_v0_file_and_persists`。）
6. **`pnpm check` 脚本不存在**（package.json 属 T1 范围，未擅改）：以等价验证替代——`pnpm lint`
   （内含 svelte-check）与 `pnpm build` 均 PASS；如主 Agent 期望补 `check` 脚本，请批准后由前端任务或主 Agent
   （T1 范围）统一增添。
7. **面板尺寸默认值 460×340**：来自 spec v0.2（对齐 design-spec），非 FR-13 表内项，但属 6.3
   示例 JSON 字段，已按 spec 实现。

## 6. 遗留与后续对接

- 命令注册与 emits（`invoke_handler`、`settings://changed` 广播剥离 apiKey）属 T13/T16 装配职责，
  events.rs 已提供全部类型；`verify_connection/set_settings` 等 invoke 参数名与 events.rs 约定一致。
- `has_key` 每次真实调用 DPAPI 解密（符合 spec 5.7.8；量级微小，不做缓存——T16 性能打点可复核）。
- crypto 测试跑在 Windows 真机真实 DPAPI 上（任务书允许）；密文不落盘（仅内存）。
- 风险：无。
