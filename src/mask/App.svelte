<script lang="ts">
  // 占位页（T1）：截图遮罩窗（label=mask-{i}，每显示器一窗）对应页面。
  // 窗口池与框选交互归 T9（冻结快照、选区挖空、complete_selection/cancel_selection）。
</script>

<svelte:head>
  <title>截图遮罩</title>
</svelte:head>

<main>
  <h1>截图遮罩</h1>
  <p>占位页 · 窗口 label=mask-i（每显示器一窗，编号由后端生成）· 遮罩与框选逻辑由 T9 实现</p>
</main>

<style>
  main {
    display: grid;
    place-content: center;
    height: 100vh;
    text-align: center;
    font-family: 'Microsoft YaHei UI', 'Microsoft YaHei', 'Segoe UI', system-ui, sans-serif;
  }
  h1 {
    font-size: 20px;
    margin: 0;
  }
  p {
    font-size: 13px;
    opacity: 0.7;
  }
</style>
