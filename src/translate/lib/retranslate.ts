// 重译按钮浮现逻辑（用户裁决 A1，2026-08-26：所有含原文区的形态可编辑重译；
// design-spec 3.3-A 「内容被修改后右下角浮现「重译 ↵」……未修改时不显示」+ FR-4.3
// 空白不得发起请求——修正后若内容为纯空白同样不浮现（无意义提交拦截）。

/** 重译按钮是否浮现：内容已被修改（≠ 已提交版本）且非纯空白。 */
export function retranslateVisible(submitted: string, current: string): boolean {
  return current !== submitted && current.trim().length > 0;
}
