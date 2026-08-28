import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const packageJson = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
const version = packageJson.version;
const exe = join(root, 'src-tauri', 'target', 'release', 'alya.exe');
if (!existsSync(exe)) {
  console.error(`未找到 ${exe}，请先运行 pnpm tauri build`);
  process.exit(1);
}

const releaseDir = join(root, 'release');
const portableDir = join(releaseDir, 'portable');
rmSync(portableDir, { recursive: true, force: true });
mkdirSync(portableDir, { recursive: true });
cpSync(exe, join(portableDir, 'Alya.exe'));
writeFileSync(
  join(portableDir, 'README.txt'),
  `Alya ${version} 便携版\r\n\r\n解压后运行 Alya.exe。Alya 需要 Windows WebView2 Runtime；若启动提示缺少运行时，请从微软官方页面安装后重试。\r\n\r\n首次运行会显示配置引导。翻译原文会发送到你配置的第三方云端服务（默认 DeepSeek）；截图识别在本机完成，截图翻译仅发送识别出的文字。\r\n\r\n本版本不包含自动更新。未签名程序首次运行可能显示 SmartScreen 提示，请确认来源后选择允许运行。\r\n`,
  'utf8',
);

const zipPath = join(releaseDir, `Alya-${version}-portable.zip`);
rmSync(zipPath, { force: true });
const portablePathForPs = portableDir.replaceAll("'", "''");
const zipPathForPs = zipPath.replaceAll("'", "''");
const psCommand = `Compress-Archive -Path '${portablePathForPs}\\*' -DestinationPath '${zipPathForPs}' -Force`;
execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', psCommand], {
  stdio: 'inherit',
});
console.log(`便携版已生成: ${zipPath}`);
