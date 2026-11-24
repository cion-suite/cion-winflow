// Запускает pnpm-команду (tauri) в окружении Visual Studio.
//   node .claude/scripts/run-vs.mjs tauri dev
//   node .claude/scripts/run-vs.mjs tauri build --debug --no-bundle
//
// tauri сам вызывает cargo, поэтому окружение должно быть выставлено
// до его старта - иначе vswhom-sys не находит заголовки и библиотеки.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { findVcvars, quote } from './vs.mjs';

const ROOT = path.resolve(import.meta.dirname, '../..');
const args = process.argv.slice(2);
if (!args.length) {
    console.error('использование: run-vs.mjs <команда> [аргументы...]');
    process.exit(1);
}

const line = `pnpm exec ${args.map(quote).join(' ')}`;
const bat = findVcvars();
const options = { cwd: ROOT, stdio: 'inherit' };
const result = process.platform === 'win32'
    ? spawnSync('cmd.exe', ['/d', '/s', '/c', bat ? `call "${bat}" >nul 2>nul && ${line}` : line], { ...options, windowsVerbatimArguments: true })
    : spawnSync('pnpm', ['exec', ...args], options);

process.exit(result.status ?? 1);
