// Общее для run-cargo: обычный Windows-шелл не знает
// INCLUDE/LIB, поэтому cargo запускается после vcvars64.bat
// установки, у которой есть настоящие x64-библиотеки.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export const quote = s => /[\s"]/.test(s) ? `"${s.replaceAll('"', '\\"')}"` : s;

// vswhere знает все установки; берём первую, у которой есть
// legacy_stdio_definitions.lib в lib/x64 (у неполной установки её нет).
export const findVcvars = () => {
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
