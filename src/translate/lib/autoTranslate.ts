// 原文变化自动翻译（真机修复轮 9，用户需求 2026-08-28）：
// 原文框内容变化（输入/粘贴）且与最近一次已提交翻译不同时，停顿 AUTO_TRANSLATE_DELAY_MS
// 后自动发起翻译，去掉"回车 / 重译按钮"必经步骤。可翻译判定复用 retranslateVisible
// （A1 裁决同源：非纯空白且 ≠ 已提交版本）。
//
// 交互规则（UX 把控）：
// - 防抖：连续输入只在停顿后触发一次，避免打字途中反复起请求；
// - 显式提交（回车/翻译/重译/送翻译）由调用方先 cancel() 再立即发起，避免双重请求；
// - 触发时面板已失焦（被隐藏或用户切走）则放弃本次——原文保留、按钮态不变，
//   避免后台空耗一次请求（回焦后可手动重译）。

import { retranslateVisible } from './retranslate';

export const AUTO_TRANSLATE_DELAY_MS = 700;

export class AutoTranslateScheduler {
  private timer: ReturnType<typeof setTimeout> | null = null;

  /** 原文输入事件驱动：满足自动翻译条件则（重新）计时，否则清除挂起任务。 */
  input(current: string, submitted: string, fire: () => void): void {
    this.cancel();
    if (!retranslateVisible(submitted, current)) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      if (!document.hasFocus()) return; // 失焦放弃（见头注）
      fire();
    }, AUTO_TRANSLATE_DELAY_MS);
  }

  /** 显式提交 / 内容被面板新事件覆盖时调用：清除挂起的自动翻译。 */
  cancel(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  get isPending(): boolean {
    return this.timer !== null;
  }
}
