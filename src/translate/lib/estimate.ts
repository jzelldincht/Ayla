// 整段模式预估剩余时间（FR-10.3 / Q3：按已生成字符近期速率外推；界面固定冠"预估"，
// 不得呈现为精确承诺；design-spec 4.3：每秒刷新、「预估剩余约 N 秒」、速率不足时只显示
// 「正在翻译…」）。
//
// 口径（实现层，spec 5.7.7 "每 500ms 按近 2 秒字符速率外推，界面固定冠预估"→ UI 每秒刷新）：
// - 速率 = 近 2 秒窗口内的字符增量 / 窗口时长（字符/秒）；
// - 总量近似 = max(源文本字符数, 已生成字符数)（译文量未可知，以源长近似——Q3 委托实现层算法）；
// - 剩余 = 总量 − 已生成；剩余 ≤0 → 0（即将完成），UI 可继续整段等待呈现；
// - 已生成字符过少（< MIN_PRODUCED）或速率过低（< MIN_RATE）→ null（仅"正在翻译…"）。

export const ETA_RATE_WINDOW_MS = 2000;
/** 已生成字符低于此值不足以估算速率（实现层阈值，design-spec 4.3"已生成字符过少"）。 */
export const ETA_MIN_PRODUCED = 16;
/** 外推速率下限（字符/秒）；低于此值视为"不足速率"。 */
export const ETA_MIN_RATE_CHARS_PER_SEC = 1;

export interface EstimateInput {
  /** 源文本字符数（本次提交的原文）。 */
  sourceLength: number;
  /** 当前已生成（缓冲）字符数。 */
  produced: number;
  /** 近 2 秒窗口实测速率（字符/秒，调用方采样）。 */
  ratePerSecond: number;
}

/**
 * 返回剩余估算秒数（向上取整，≥0）；不足以估算 → null（显示"正在翻译…"）。
 */
export function computeEtaSeconds(input: EstimateInput): number | null {
  if (input.produced < ETA_MIN_PRODUCED) return null;
  if (input.ratePerSecond < ETA_MIN_RATE_CHARS_PER_SEC) return null;
  const total = Math.max(input.sourceLength, input.produced);
  const remaining = total - input.produced;
  if (remaining <= 0) return 0;
  return Math.max(1, Math.ceil(remaining / input.ratePerSecond));
}

/** 标定文案：「预估剩余约 N 秒」（"预估"务必独立成词且醒目——由组件层着色，非承诺语义）。 */
export function formatEtaLabel(seconds: number): string {
  return `预估剩余约 ${seconds} 秒`;
}
