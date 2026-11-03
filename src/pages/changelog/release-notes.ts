export type ReleaseEntry = {
  version: string;
  bullets: string[];
};

const SECTION = /^###\s+(?:Версия\s+)?v?(\d[\w.-]*)\s*$/;
const BULLET = /^-\s+(.+)$/;

/** Разбирает docs/versions/release.md: раздел версии и его буллеты. */
export function parseReleaseNotes(markdown: string): ReleaseEntry[] {
  const entries: ReleaseEntry[] = [];

  for (const line of markdown.split(/\r?\n/)) {
    const heading = line.match(SECTION);
    if (heading) {
      entries.push({ version: heading[1], bullets: [] });
      continue;
    }
    const bullet = line.match(BULLET);
    const current = entries.at(-1);
    if (bullet && current) current.bullets.push(bullet[1].trim());
  }

  return entries;
}

// glob, а не прямой импорт: удалённый лог не должен ломать сборку шаблона
const FILES = import.meta.glob<string>("../../../docs/versions/release.md", {
  query: "?raw",
  import: "default",
  eager: true,
});

export const RELEASES = parseReleaseNotes(Object.values(FILES)[0] ?? "");
