# Alya 开发约定（AI 辅助开发补充）

## Windows 命令输出编码

本机控制台代码页为 936 (GBK)。`taskkill`、`reg`、`netsh` 等输出本地化中文的 Windows 命令，GBK 字节会被 ZCode 任务输出按 UTF-8 解码成乱码，污染任务输出与日志。ZCode 的 Bash 是非登录、非交互 shell，不加载 `~/.bashrc`，profile 层修复无效。

- 需要此类命令时用代码页包裹：`cmd //c "chcp 65001>nul & taskkill /F /IM alya.exe"`
- 或改用 PowerShell：`powershell -NoProfile -Command "Stop-Process -Name alya -Force -ErrorAction SilentlyContinue"`
- 构建前需结束运行中的 Alya 实例时，按上式执行，不要裸调 `taskkill`。
