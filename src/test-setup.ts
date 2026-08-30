// vitest 全局 setup（i18n-1）：经 globalThis 传递测试语言锁定标记。
// 既有断言文案均为中文，而语言默认 'system'——jsdom 的 navigator.language 是 en-US，
// 组件会解析到英文目录导致大面积断言失败。注意：本文件严禁 import 项目模块——
// setup 先于各测试文件的 vi.mock 执行，会把真实 ipc 模块缓存进 stores（invoke undefined）。
(globalThis as Record<string, unknown>).__ALYA_TEST_LOCALE__ = 'zh-CN';
