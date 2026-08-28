import { readFileSync } from 'node:fs';

const packageJson = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
const cargo = readFileSync(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8');
const tauri = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const versions = { package: packageJson.version, cargo: cargoVersion, tauri: tauri.version };
const values = Object.values(versions);
if (!values.every((value) => value === values[0])) {
  console.error('版本不一致:', versions);
  process.exit(1);
}
if (!/^\d+\.\d+\.\d+$/.test(values[0])) {
  console.error(`发布版本必须是 SemVer x.y.z，当前为 ${values[0]}`);
  process.exit(1);
}
console.log(`版本一致: ${values[0]}（package.json / Cargo.toml / tauri.conf.json）`);
