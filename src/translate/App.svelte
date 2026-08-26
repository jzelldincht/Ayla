<script lang="ts">
  // 占位页（T1）：翻译面板窗口（label=translate）对应页面。
  // 窗口真实创建与生命周期归 T11（启动序列末尾预创建并隐藏，D5 暖窗）；形态与错误态 UI 归 T12。
</script>

<svelte:head>
  <title>翻译面板</title>
</svelte:head>

<main>
  <h1>翻译面板</h1>
  <p>占位页 · 窗口 label=translate · 本页面由 T11 挂载到真实窗口</p>
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
