<script lang="ts">
  // 占位页（T1）：首启引导窗（label=onboarding）对应页面。
  // 创建条件检测（无已存配置）与 guide 模式复用归 T14（windows/onboarding.rs）；四步向导 UI 归 T14 前端。
</script>

<svelte:head>
  <title>欢迎使用 Alya</title>
</svelte:head>

<main>
  <h1>欢迎使用 Alya</h1>
  <p>占位页 · 窗口 label=onboarding · 首次运行引导由 T14 实现</p>
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
