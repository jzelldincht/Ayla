//! clipboard_monitor：剪贴板监听状态机——GetClipboardSequenceNumber 500ms 轮询、
//! 内容去重、抑制窗与超限跳过（T5 实现，spec 5.6.3）。
