// 滚动锚定（FR-10.4 / design-spec 4.3 流式输出规则）：
// 用户未向上滚动时自动贴底跟随；用户上滚超过一行高度即停止自动跟随；滚回底部后恢复跟随
// （不设悬浮按钮，P3 克制）。阈值：距底 <24px（逻辑 px）视为"贴底"。
//
// 纯逻辑（可单测）：update() 记当前滚动状态，shouldFollow() 返回是否应跟随；
// applyAppend() 在"仍跟随"时把容器滚回底部（scrollTop=scrollHeight）。

export const ANCHOR_THRESHOLD_PX = 24;

export class ScrollAnchor {
  private following = true;

  /** 在滚动/内容变化事件中调用；返回当前是否处于跟随态。 */
  update(scrollTop: number, scrollHeight: number, clientHeight: number): boolean {
    const distance = scrollHeight - clientHeight - scrollTop;
    this.following = distance < ANCHOR_THRESHOLD_PX;
    return this.following;
  }

  get isFollowing(): boolean {
    return this.following;
  }

  /** 内容追加后调用：跟随态则贴底滚动（把新内容滚入视口）。 */
  applyAppend(el: HTMLElement): void {
    if (this.following) {
      el.scrollTop = el.scrollHeight;
    }
  }

  /** 测试/重态：显式复位为跟随态。 */
  reset(): void {
    this.following = true;
  }
}
