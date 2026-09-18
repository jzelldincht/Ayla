// OCR 识别文本轻量后处理（清单 P2-8）。
//
// 目标：修正 OCR 引擎输出的常见可读性问题，同时保守（不确定就不动，保留可编辑性）：
// 1) 行尾连字符断词合并：拉丁词被断行（`win-\ndow`）→ 去连字符直连；
// 2) 行间合并：一句话被拆成多行 → 行末非句末标点时并为一行（CJK 相邻无空格，拉丁相邻单空格）；
//    空行 = 段落边界，跨段不合并；
// 3) CJK 上下文中的下划线误识：`—`（破折号）常被识别为 `_` → CJK 相邻的 `_`/`__` 归一为 `——`。
// 不做：单词拼写纠错、全角/半角批量转换（风险大于收益，交给用户编辑或翻译模型）。

/** 是否 CJK（汉字/假名/谚文）。 */
function isCjk(ch: string): boolean {
  return /[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af\uff01-\uff5e]/.test(ch);
}

/** 是否句末/结构标点（此处断行属段落结构，不合并）。 */
function endsSentence(line: string): boolean {
  return /[.。!！?？…:;；,，]$/.test(line.trimEnd());
}

/** 行尾连字符断词：`xxx-` + 下一行以拉丁字母开头 → 去连字符合并。 */
function joinHyphenated(lines: string[]): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < lines.length) {
    const cur = lines[i];
    const next = i + 1 < lines.length ? lines[i + 1] : null;
    if (
      next !== null &&
      /[A-Za-z]-$/.test(cur.trimEnd()) &&
      /^[a-z]/.test(next.trimStart())
    ) {
      out.push(cur.trimEnd().slice(0, -1) + next.trimStart());
      i += 2;
    } else {
      out.push(cur);
      i += 1;
    }
  }
  return out;
}

/** 行间合并：空行分段；段内行末非句末标点 → 合并（CJK 相邻无空格，否则单空格）。 */
function joinLines(lines: string[]): string {
  const out: string[] = [];
  let buf = '';
  const flush = (): void => {
    if (buf.trim().length > 0) out.push(buf.trimEnd());
    buf = '';
  };
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (line.trim().length === 0) {
      flush();
      out.push('');
      continue;
    }
    if (buf.length === 0) {
      buf = line;
      continue;
    }
    const prevEnd = buf.trimEnd().slice(-1);
    const nextStart = line.trimStart().slice(0, 1);
    const cjkAdjacency = isCjk(prevEnd) || isCjk(nextStart);
    if (endsSentence(buf)) {
      flush();
      buf = line;
    } else {
      buf += cjkAdjacency ? line : ` ${line.trimStart()}`;
    }
  }
  flush();
  return out.join('\n');
}

/** OCR 文本规整主入口（保守；结果仍可编辑）。 */
export function normalizeOcrText(raw: string): string {
  if (raw.length === 0) return raw;
  const normalized = raw.replace(/\r\n?/g, '\n');
  const joined = joinLines(joinHyphenated(normalized.split('\n')));
  // CJK 相邻的 1~2 个 `_` → 破折号误识归一（拉丁上下文中的 `_` 常为合法字符，不动）
  return joined.replace(
    /(?<=[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af])\s*_{1,2}\s*(?=[\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af])/g,
    '——',
  );
}
