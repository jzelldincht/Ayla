//! windows：四窗口生命周期与定位（spec 5.5）。
//!
//! 本骨架**不静态预创建**任何窗口（tauri.conf.json app.windows 为空）：
//! 各窗口的创建时机为按需/条件式（spec 5.5 生命周期列），由对应任务经 Rust 动态创建：
//! - `translate`：无边框、透明（CSS 圆角）、阴影、skip_taskbar、topmost 于 show 置位/
//!   失焦隐藏后清除（C3 裁决）、可拖边缘调尺寸、隐藏≠销毁 —— T11 启动序列末尾预创建并隐藏（D5 暖窗）；
//! - `mask-{i}`：每显示器一窗，无边框、不透明（自绘冻结快照）、skip_taskbar、topmost、
//!   全屏覆盖所在显示器，池化复用、拓扑变化销毁重建 —— T9；
//! - `settings`：有边框、可调尺寸、单例、关闭=隐藏，首次打开按需创建 —— T13；
//! - `onboarding`：固定尺寸向导、有边框；启动检测无任何已存配置则创建并显示，
//!   已存在有效配置永不自动弹出（FR-11.5），guide 模式复用 —— T14。
//!
//! 定位与坐标约束（INV-4）：跨进程与截屏坐标一律物理像素，面板持久化尺寸一律逻辑像素。

pub mod mask;
pub mod onboarding;
pub mod settings;
pub mod translate;
