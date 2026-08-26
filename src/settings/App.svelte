<script lang="ts">
  // 占位页（T1）：设置窗（label=settings）对应页面。
  // 窗口按需创建/隐藏复用归 T13（windows/settings.rs）；表单与即时保存归 T13 前端。
</script>

<svelte:head>
  <title>设置</title>
</svelte:head>

<main>
  <h1>设置</h1>
  <p>占位页 · 窗口 label=settings · 设置表单由 T13 实现</p>
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
