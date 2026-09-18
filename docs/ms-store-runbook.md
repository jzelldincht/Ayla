# Microsoft Store 提交 Runbook（Alya / Windows）

> 状态：技术准备完成（MSIX 打包 + 本地安装实测通过）。账号注册与商店页素材填写需人工完成。

## 0. 前置条件（人工）

1. **Partner Center 账户**：<https://partner.microsoft.com/dashboard> → 注册 Individual 个人开发者，
   一次性 $19（需微软账号 + 实名/身份验证，1~2 天审核）。
2. 账户通过后，在 Partner Center → Account settings 里拿到**保留的发布者标识**
   （形如 `CN=XXXXXXX-XXXX-...`）——打包时用 `-Publisher` 传入。

## 1. 打包（本仓库已就绪）

```powershell
# 1) 构建 release（前端 + Rust）
pnpm tauri build

# 2) 打 MSIX（-Sign 产生自签名包用于本机验证；上架包无需签名——Store 重新签名）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-msix.ps1 `
  -Publisher "CN=<Partner Center 分配的发布者标识>" `
  -IdentityName "Jzelldincht.Ayla"
# 输出：build\msix\Alya_<version>_x64.msix
```

要点：
- 未签名本地测试包的 Identity Name **必须以 `.unsigned` 结尾**（`-AllowUnsigned` 保留命名空间，
  错误 0x80073D2C 即此规则）；正式上架包用 `Jzelldincht.Ayla` + Store 签名。
- 未签名含可执行内容的包安装**需管理员 PowerShell**（脚本 `install-msix-admin.ps1` 经 UAC 提权）。

## 2. 本地验证清单（已完成项打勾）

- [x] MakeAppx 打包通过清单校验（DefaultTile 需 Wide310x150 或省略 310x310——已省略）
- [x] Add-AppxPackage -AllowUnsigned 安装成功（PFN=`Jzelldincht.Ayla_qrvg5e1dwgepr`）
- [x] AUMID 启动（`shell:AppsFolder\<PFN>!Ayla`）→ 进程来自 `C:\Program Files\WindowsApps\...`
- [x] StartupTask 扩展注册（`State=0` 默认禁用；系统设置>应用>启动 可见可开）
- [x] 配置写入行为：包身份实例写入**真实** `%APPDATA%\com.alya.app`（虚拟化未重定向，
      实测 2026-09-18）→ 老用户配置天然迁移，无需导入
- [ ] 人工：在已安装的 Store 版里验证 设置 > 通用 > 开机自启 切换（走 StartupTask API）：
      开启 → 任务管理器 > 启动应用 出现 Alya；关闭 → 消失
- [ ] 人工：托盘/热键/划词/截图四链路冒烟（包身份下 RegisterHotKey 与剪贴板事件预期正常）

## 3. Partner Center 提交

1. **App 创建**：Apps and Games → New product → Name：`Alya`（或 `Alya — AI Translator`）。
2. **定价**：Free（第一版）；买断制上线时改为 Price + Free trial 组合。
3. **属性**：Category = Productivity 或 Utilities；Declared age rating 问卷（无暴力/无不良内容 → 3+/Everyone）。
4. **Packages**：上传 `Alya_<version>_x64.msix`。微软会自动重新签名（免代码签名证书）。
5. **Store listing**：`store/listing.md` 已备九语文案（每语言：名称/简介/描述/功能点）。
   语言在 Partner Center 的 Manage languages 里逐个添加后粘贴。
6. **截图**：1366×768 至少 1 张 + 推荐 1920×1080（注意左缘裁 8px 隐形边框、裁 4px 面板，
   见 docs/UI优化清单.md「已在本轮修复」节）。
7. **必需 URL**：Privacy policy = 隐私政策页公网地址（`website/privacy-policy.html` 发布到
   GitHub Pages 后的链接）；Support = GitHub 仓库 Issues 页。
8. **提交认证**：自动扫描 + 人工抽检，1~3 天。常见拒因：隐私政策缺失、声明功能不符、崩溃。

## 4. 已知行为与边界（记录备查）

- **配置共享**：包身份实例与便携/安装版共用真实 `%APPDATA%\com.alya.app`（实测未走容器）。
  好处：换渠道零迁移；边界：两渠道**同时**运行会争写同一配置（最后写入者胜）——
  文档提示用户二选一即可。
- **开机自启双模式**：包身份 → WinRT StartupTask（manifest `uap5:startupTask` +
  `Windows.ApplicationModel.StartupTask` API）；非包 → HKCU Run（现状）。
  Store 版在 Windows 更新后启动路径由系统管理，不会失效。
- **卸载**：开始菜单右键卸载，或 `Get-AppxPackage Jzelldincht.Ayla | Remove-AppxPackage`。
