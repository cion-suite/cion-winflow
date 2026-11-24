// Правила CLAUDE.md и docs/architecture.md в исполняемом виде.
// Единый источник для diff-scan.mjs и plan-scan.mjs - иначе чек-листы
// команд разъезжаются. Самопроверка: node .claude/scripts/rules.mjs
//
// tier C (confirmed) - regex и есть истина, триаж не нужен.
// tier K (check)     - кандидат, вердикт только с контекстом файла.
// Механику, которую уже ловит ESLint/clippy (границы слоёв, any,
// форматирование Rust), здесь не дублируем.

const TS = /\.(ts|tsx|js|mjs)$/;
const RS = /\.rs$/;
const COMMENT = /^\s*(?:\/\/|\/\*|\*|#)/;
const TEST = /(^|\/)(tests?|fixtures)\/|\.(test|spec)\./;

const HTTP_HOME = 'src/api/client.ts';
const SETTINGS_HOMES = new Set(['src/lib/settings.ts', 'src/shell/navbar/ThemeToggle.tsx']);

const HEX_CLASS = /\b(?:bg|text|border|ring|fill|stroke|from|via|to)-\[#[0-9a-fA-F]{3,8}\]/;
const TS_ANY = /@ts-(?:ignore|nocheck)\b/;
const AS_ANY = /\bas\s+any\b/;
const RUST_UNSAFE = /\b(?:unwrap|expect)\s*\(|\bunsafe\b/;
const RUST_PANIC = /\b(?:panic!|todo!|unimplemented!)\s*\(/;

// WHY-комментарий строкой выше снимает вопрос "а объяснил?"
const whyAbove = prev => COMMENT.test(prev ?? '');

const tsAny = t => TS_ANY.test(t) || (!COMMENT.test(t) && AS_ANY.test(t));

export const RULES = [
    {
        id: '1.1', sev: '🔴', tier: 'C', name: 'fetch только в src/api/client.ts',
        hit: (t, f) => TS.test(f) && f.startsWith('src/') && !TEST.test(f) && f !== HTTP_HOME
            && !COMMENT.test(t) && /\bfetch\s*\(/.test(t),
    },
    {
        id: '1.2', sev: '🔴', tier: 'C', name: 'localStorage/sessionStorage только в settings.ts и ThemeToggle.tsx',
        hit: (t, f) => TS.test(f) && f.startsWith('src/') && !TEST.test(f) && !SETTINGS_HOMES.has(f)
            && !COMMENT.test(t) && /\b(?:local|session)Storage\s*\.\s*(?:getItem|setItem|removeItem|clear)\s*\(/.test(t),
    },
    {
        id: '1.4', sev: '🟡', tier: 'C', name: 'цвет Tailwind задан вручную вместо токена темы',
        hit: (t, f) => TS.test(f) && f.startsWith('src/') && !COMMENT.test(t) && HEX_CLASS.test(t),
    },
    {
        id: '1.5', sev: '🟡', tier: 'C', name: 'Button Base UI: asChild вместо render',
        hit: (t, f) => f.startsWith('src/') && f.endsWith('.tsx') && !COMMENT.test(t) && /\basChild\b/.test(t),
    },
    {
        id: '1.6', sev: '🟡', tier: 'K', name: '@ts-ignore / as any без WHY-комментария',
        hit: (t, f, prev) => f.startsWith('src/') && TS.test(f) && !TEST.test(f)
            && tsAny(t) && !whyAbove(prev),
    },
    {
        id: '1.7', sev: '🟡', tier: 'K', name: 'unwrap/expect/unsafe в Rust без WHY-комментария',
        // build.rs - программа сборки: panic/expect здесь и есть отчёт об ошибке.
        hit: (t, f, prev) => RS.test(f) && !TEST.test(f) && !/(^|\/)build\.rs$/.test(f)
            && !COMMENT.test(t) && RUST_UNSAFE.test(t) && !whyAbove(prev),
    },
    {
        id: '1.8', sev: '🔴', tier: 'K', name: 'panic!/todo!/unimplemented! в Rust',
        hit: (t, f) => RS.test(f) && !TEST.test(f) && !COMMENT.test(t) && RUST_PANIC.test(t),
    },
];

// shadcn - вендорный генерат: его правки ревьюятся только глазами.
const SKIP = /^src\/ui\/shadcn\//;
export const applies = f => (TS.test(f) || RS.test(f)) && !SKIP.test(f) && !/^\.claude\//.test(f);

// Единственная точка применения текстовых правил;
// prev - ближайшая непустая строка выше.
export const match = (text, file, prev) => applies(file)
    ? RULES.filter(r => !r.path && r.hit(text, file, prev))
    : [];

// По пути - только внесённое; from - путь до переименования.
export const matchPath = (file, from) => {
    const hits = f => applies(f) ? RULES.filter(r => r.path && r.hit('', f)) : [];
    const old = new Set(from ? hits(from).map(r => r.id) : []);
    return hits(file).filter(r => !old.has(r.id));
};

// Ближайшая непустая строка выше из хунка - для WHY-исключений.
const above = (f, n) => {
    for (let i = n - 1; f.post.has(i); i--) if (f.post.get(i).trim()) return f.post.get(i);
};

// Файл из git.diff: добавленные строки и путь у A и R (без line).
export const matchDiff = f => [
    ...f.added.flatMap(a => match(a.text, f.path, above(f, a.line)).map(r => ({ ...r, file: f.path, line: a.line }))),
    ...(f.status === 'A' || f.status === 'R' ? matchPath(f.path, f.from).map(r => ({ ...r, file: f.path })) : []),
];

// [id, файл, нарушение, норма, файл нормы?]
const CASES = [
    ['1.1', 'src/pages/a/index.tsx', 'const r = await fetch("/api")', 'const r = await api.get()'],
    ['1.1', 'src/pages/a/index.tsx', 'const r = await fetch("/api")', 'const r = await fetch("/api")', 'src/api/client.ts'],
    ['1.2', 'src/pages/a/index.tsx', 'localStorage.getItem("k")', 'settings.get("locale")'],
    ['1.2', 'src/pages/a/index.tsx', 'localStorage.getItem("k")', 'localStorage.getItem("k")', 'src/lib/settings.ts'],
    ['1.4', 'src/pages/a/index.tsx', '<div className="bg-[#fff]" />', '<div className="bg-background" />'],
    ['1.5', 'src/pages/a/index.tsx', '<Button asChild>', '<Button render={<a />} nativeButton={false}>'],
    ['1.6', 'src/pages/a/index.tsx', '// @ts-ignore', '// обычный комментарий'],
    ['1.6', 'src/pages/a/index.tsx', 'const x = value as any', 'const x = value as string'],
    ['1.7', 'server/src/main.rs', 'let x = value.unwrap();', 'let x = value?;'],
    ['1.7', 'server/src/main.rs', 'let x = value.unwrap();', '// WHY: инвариант проверен выше\nlet x = value.unwrap();'],
    ['1.7', 'server/src/main.rs', 'let x = value.unwrap();', 'let out = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");', 'server/build.rs'],
    ['1.8', 'server/src/main.rs', 'panic!("boom")', 'return Err(AppError::Internal);'],
    ['1.8', 'server/src/main.rs', 'todo!("later")', '// TODO: реализовать в фазе 2'],
];

const fires = (id, text, file) => {
    const [prev, line] = text.includes('\n') ? text.split('\n') : [undefined, text];
    return [...match(line, file, prev), ...matchPath(file)].some(r => r.id === id);
};

export function selftest() {
    let bad = 0;
    for (const [id, file, violation, clean, cleanFile = file] of CASES) {
        if (!fires(id, violation, file)) { console.error(`FAIL §${id}: не сработало на "${violation}" (${file})`); bad++; }
        if (fires(id, clean, cleanFile)) { console.error(`FAIL §${id}: ложное срабатывание на "${clean}" (${cleanFile})`); bad++; }
        if (match(violation, 'CLAUDE.md').length) { console.error(`FAIL §${id}: сработало на .md`); bad++; }
    }
    const lost = RULES.filter(r => !CASES.some(c => c[0] === r.id));
    for (const r of lost) { console.error(`FAIL §${r.id}: нет пары в CASES`); bad++; }
    console.log(bad ? `SELFTEST: ${bad} провалов` : `SELFTEST: ok, ${RULES.length} правил, ${CASES.length} пар`);
    return bad ? 1 : 0;
}

export function scan(lines, file) {
    const out = [];
    let prev;
    lines.forEach((text, i) => {
        for (const r of match(text, file, prev)) out.push({ file, line: i + 1, text, ...r });
        if (text.trim()) prev = text;
    });
    return out;
}

if (process.argv[1]?.endsWith('rules.mjs')) process.exit(selftest());
