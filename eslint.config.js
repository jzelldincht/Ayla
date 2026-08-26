import js from '@eslint/js';
import prettierConfig from 'eslint-config-prettier';
import eslintPluginSvelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

export default ts.config(
  {
    ignores: ['**/node_modules/**', '**/dist/**', '**/src-tauri/**', '**/gen/**'],
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...eslintPluginSvelte.configs['flat/recommended'],
  prettierConfig,
  {
    files: ['**/*.svelte', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: {
        svelteConfig,
      },
    },
  },
  {
    files: ['**/*.{js,mjs,ts,svelte}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
);
