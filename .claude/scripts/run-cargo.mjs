// cargo с окружением Visual Studio на Windows: обычный shell не знает
// INCLUDE/LIB, и линковка падает на LNK1181 legacy_stdio_definitions.lib.
// Скрипт находит установку с настоящими x64-библиотеками и вызывает
// cargo после vcvars64.bat. На Linux/macOS просто запускает cargo.
//
//   node .claude/scripts/run-cargo.mjs test
//   node .claude/scripts/run-cargo.mjs fmt --check
//   node .claude/scripts/run-cargo.mjs clippy --all-targets -- -D warnings
//
// --manifest-path подставляется сам: сервер — единственный Rust-крейт.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { findVcvars, quote } from './vs.mjs';

const ROOT = path.resolve(import.meta.dirname, '../..');
const args = process.argv.slice(2);
const manifest = 'server/Cargo.toml';
// --manifest-path - опция cargo, а не rustc: после `--` она ушла бы
// в clippy/rustc и cargo не нашёл бы манифест.
const sep = args.indexOf('--');
const withManifest = sep < 0
    ? [...args, '--manifest-path', manifest]
    : [...args.slice(0, sep), '--manifest-path', manifest, ...args.slice(sep)];
const full = args.includes('--manifest-path') ? args : withManifest;

const bat = findVcvars();
const line = `cargo ${full.map(quote).join(' ')}`;
const options = { cwd: ROOT, stdio: 'inherit' };
// windowsVerbatimArguments: cmd.exe понимает кавычки по-своему,
// обычное экранирование Node превращает их в \"path\".
const result = bat
    ? spawnSync('cmd.exe', ['/d', '/s', '/c', `call "${bat}" >nul 2>nul && ${line}`], { ...options, windowsVerbatimArguments: true })
    : spawnSync('cargo', full, options);

process.exit(result.status ?? 1);
