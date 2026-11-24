// cargo с окружением Visual Studio на Windows: обычный shell не знает
// INCLUDE/LIB, и линковка падает на LNK1181 legacy_stdio_definitions.lib.
// Скрипт находит установку с настоящими x64-библиотеками и вызывает
// cargo после vcvars64.bat. На Linux/macOS просто запускает cargo.
//
//   node .claude/scripts/run-cargo.mjs test
//   node .claude/scripts/run-cargo.mjs fmt --check
//   node .claude/scripts/run-cargo.mjs clippy --all-targets -- -D warnings
//
// --manifest-path подставляется сам: server/Cargo.toml, пока его нет -
// src-tauri/Cargo.toml.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = path.resolve(import.meta.dirname, '../..');
const args = process.argv.slice(2);
const manifest = fs.existsSync(path.join(ROOT, 'server/Cargo.toml'))
    ? 'server/Cargo.toml'
    : 'src-tauri/Cargo.toml';
// --manifest-path - опция cargo, а не rustc: после `--` она бы ушла
// в clippy/rustc и cargo не нашёл бы манифест.
const sep = args.indexOf('--');
const withManifest = sep < 0
    ? [...args, '--manifest-path', manifest]
    : [...args.slice(0, sep), '--manifest-path', manifest, ...args.slice(sep)];
const full = args.includes('--manifest-path') ? args : withManifest;

const quote = s => /[\s"]/.test(s) ? `"${s.replaceAll('"', '\\"')}"` : s;

// vswhere знает все установки; берём первую, у которой есть
// legacy_stdio_definitions.lib в lib/x64 (у неполной установки её нет).
const vcvars = () => {
    if (process.platform !== 'win32' || process.env.VCINSTALLDIR) return null;
    const programFilesX86 = process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)';
    const vswhere = path.join(programFilesX86, 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
    if (!fs.existsSync(vswhere)) return null;
    const found = spawnSync(vswhere, ['-products', '*', '-format', 'json'], { encoding: 'utf8' });
    if (found.status !== 0) return null;
    let installs;
    try { installs = JSON.parse(found.stdout.replace(/^\uFEFF/, '')); } catch { return null; }
    for (const inst of installs) {
        const bat = path.join(inst.installationPath, 'VC', 'Auxiliary', 'Build', 'vcvars64.bat');
        const tools = path.join(inst.installationPath, 'VC', 'Tools', 'MSVC');
        if (!fs.existsSync(bat) || !fs.existsSync(tools)) continue;
        const complete = fs.readdirSync(tools).some(v =>
            fs.existsSync(path.join(tools, v, 'lib', 'x64', 'legacy_stdio_definitions.lib')));
        if (complete) return bat;
    }
    return null;
};

const bat = vcvars();
const line = `cargo ${full.map(quote).join(' ')}`;
const options = { cwd: ROOT, stdio: 'inherit' };
// windowsVerbatimArguments: cmd.exe понимает кавычки по-своему,
// обычное экранирование Node превращает их в \"path\".
const result = bat
    ? spawnSync('cmd.exe', ['/d', '/s', '/c', `call "${bat}" >nul 2>nul && ${line}`], { ...options, windowsVerbatimArguments: true })
    : spawnSync('cargo', full, options);

process.exit(result.status ?? 1);
