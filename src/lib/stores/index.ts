// 全局 stores（占位）：T12/T13 在此落面板侧监听开关镜像、活动场景与主题状态。
// 约定（INV-8）：store 只保存「前端展示态镜像」，真实状态以 Rust config 为单一事实源，
// 经 `settings://changed` 事件同步，禁止前端自行改状态。
export {};
