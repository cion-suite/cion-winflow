// Общее для скриптов .claude/scripts: файлы репозитория, git и дифф, кеш.
// Новый помощник клади сюда, в его namespace, а не копией в скрипт.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

export const ROOT = path.resolve(import.meta.dirname, '../..');

export const die = msg => { console.error(`ОТКАЗ: ${msg}`); process.exit(1); };

// -- git

// core.quotepath=false: иначе кириллица в путях приходит
// октальными escape'ами и ломает разбор
const run = (...args) => {
    try {
        return execFileSync('git', ['-c', 'core.quotepath=false', ...args],
            { cwd: ROOT, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (e) {
        return die(`git ${args.join(' ')} - ${String(e.stderr || e.message).trim()}`);
    }
};

// Порог явный: от умолчаний git зависят пары переименований.
// -l0: сверх renameLimit перенос выглядит как D + A
const RENAMES = ['-M30%', '-l0'];
// Префиксы явные: diff.noprefix и diff.mnemonicPrefix
// в конфиге ломают разбор путей
const DIFF = ['diff', '--no-color', '--no-ext-diff', ...RENAMES, '--src-prefix=a/', '--dst-prefix=b/'];

// путь с пробелом git закрывает в `---`/`+++` табуляцией:
// без среза `.ts\t` выпадает из правил и repo.kind
const unquote = p => p.replace(/\t$/, '').replace(/^"|"$/g, '');

const parseFile = chunk => {
    const f = { path: '', status: 'M', bin: false, plus: 0, minus: 0, added: [], removed: [], hunks: [], patch: chunk };
    let n = 0, inHunk = false, from = '', to = '';
    for (const ln of chunk.split(/\r?\n/)) {
        if (ln.startsWith('@@')) {
            inHunk = true;
            n = Number(ln.match(/\+(\d+)/)?.[1] ?? 0);
            f.hunks.push(ln.replace(/^@@[^@]*@@\s?/, ''));
        } else if (!inHunk) {
            if (ln.startsWith('+++ ')) to = unquote(ln.slice(4));
            else if (ln.startsWith('--- ')) from = unquote(ln.slice(4));
            else if (ln.startsWith('new file')) f.status = 'A';
            else if (ln.startsWith('deleted file')) f.status = 'D';
            else if (ln.startsWith('rename from ')) { f.status = 'R'; f.from = unquote(ln.slice(12)); }
            // у переименования без правок нет `---`/`+++`, а путь
            // с ` b/` из `diff --git` не вынуть
            else if (ln.startsWith('rename to ')) to = 'b/' + unquote(ln.slice(10));
            else if (ln.startsWith('Binary files')) f.bin = true;
        } else if (ln.startsWith('+')) {
            f.added.push({ line: n++, text: ln.slice(1) });
            f.plus++;
        } else if (ln.startsWith('-')) {
            f.removed.push(ln.slice(1));
            f.minus++;
        } else if (ln.startsWith(' ')) n++;
    }
    const head = chunk.slice(0, chunk.indexOf('\n'));
    f.path = to && to !== '/dev/null' ? to.slice(2)
        : from && from !== '/dev/null' ? from.slice(2)
        : unquote(head.slice(head.indexOf(' b/') + 3));
    return f;
};

const parse = text => text.split(/^(?=diff --git )/m).filter(c => c.startsWith('diff --git ')).map(parseFile);

// `git diff` untracked не видит - синтезируем патч сами
// (`--no-index` спотыкается о /dev/null на Windows)
const untracked = paths => run('ls-files', '--others', '--exclude-standard', '-z', '--', ...paths)
    .split('\0').filter(Boolean)
    .map(f => {
        let body;
        try { body = fs.readFileSync(path.join(ROOT, f), 'utf8'); } catch { return ''; }
        const head = `diff --git a/${f} b/${f}\nnew file mode 100644\n`;
        if (body.includes('\0')) return `${head}Binary files /dev/null and b/${f} differ\n`;
        if (!body) return head;
        const lines = body.replace(/\r?\n$/, '').split(/\r?\n/);
        return `${head}--- /dev/null\n+++ b/${f}\n@@ -0,0 +1,${lines.length} @@\n${lines.map(l => '+' + l).join('\n')}\n`;
    })
    .join('');

export const git = {
    run,
    // Без ревизий - `git diff HEAD` + untracked. Пути после `--`;
    // исключение: `-- . ':(exclude)<путь>'` и для untracked
    diff(args = []) {
        const sep = args.indexOf('--');
        const revs = sep < 0 ? args : args.slice(0, sep);
        const paths = sep < 0 ? [] : args.slice(sep + 1);
        const mode = revs.length ? 'range' : 'uncommitted';
        const text = mode === 'range'
            ? run(...DIFF, ...args)
            : run(...DIFF, 'HEAD', '--', ...paths) + untracked(paths);
        const spec = mode === 'range' ? args.join(' ') : `HEAD + untracked${paths.length ? ` -- ${paths.join(' ')}` : ''}`;
        return { mode, spec, revs, paths, text, files: parse(text) };
    },
};

// -- файлы репозитория

// shadcn - вендорный генерат: дайджест его не читает
const VENDORED = /^src\/ui\/shadcn\//;
const NOISE = /(^|\/)(pnpm-lock\.yaml|package-lock\.json|yarn\.lock|Cargo\.lock)$|^docs\/versions\/|\.(png|jpe?g|gif|ico|icns|svg|woff2?|ttf|exe|dll|zip)$/i;
const DOC = /\.(md|txt)$/i;

export const repo = {
    exists: f => fs.existsSync(path.join(ROOT, f)),
    read: f => { try { return fs.readFileSync(path.join(ROOT, f), 'utf8'); } catch { return ''; } },
    // путь лежит в каталоге или сам им является
    under: (p, dir) => p === dir || p.startsWith(dir + '/'),
    // что значит файл для дайджеста: код, проза или шум
    kind: f => NOISE.test(f) || VENDORED.test(f) ? 'noise' : DOC.test(f) ? 'doc' : 'code',
};

// -- кеш

export const cache = {
    // Каталог сносится при открытии, создаётся при записи:
    // старый отчёт не прочитать. keep - для шага после записи
    open(name, { keep = false } = {}) {
        const rel = `.claude/cache/${name}`;
        const dir = path.join(ROOT, rel);
        if (!keep) fs.rmSync(dir, { recursive: true, force: true });
        const at = file => {
            fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
            return path.join(dir, file);
        };
        return {
            rel,
            write: (file, body) => { fs.writeFileSync(at(file), body, 'utf8'); return `${rel}/${file}`; },
        };
    },
};
