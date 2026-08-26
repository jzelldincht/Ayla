import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// 类型化 IPC 包装（占位）：T3 依 spec 6.1 命令契约填充具体命令名与参数类型。
// 当前仅提供统一入口；骨架期不放任何业务命令（本任务不实现业务逻辑）。

/** 调用后端命令（骨架占位：命令名与返回类型由 T3 按 6.1 契约填充）。 */
export function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

/** 订阅后端事件（骨架占位：事件名与 payload 类型由 T3 按 6.2 契约填充）。 */
export async function subscribe<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  const unlisten = await listen<T>(event, (e) => handler(e.payload));
  return () => unlisten();
}
