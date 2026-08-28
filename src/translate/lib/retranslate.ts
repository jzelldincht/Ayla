// 重译按钮浮现逻辑（用户裁决 A1，2026-08-26：所有含原文区的形态可编辑重译）+ 自动翻译触发判定
// （真机修复轮 9 复用同源）；design-spec 3.3-A + FR-4.3 空白不得发起请求。
// 真机修复轮 11（用户反馈）：判定按 trim 后比较——首尾空白变化不算实质修改，
// 自动翻译不触发、重译按钮不浮现（翻译结果与首尾空白无关，浮现/触发只是打扰）。

/** 是否需要（重）翻译：trim 后非空白且 ≠ 已提交版本（忽略首尾空白差异）。 */
export function retranslateVisible(submitted: string, current: string): boolean {
  const next = current.trim();
  return next.length > 0 && next !== submitted.trim();
}
