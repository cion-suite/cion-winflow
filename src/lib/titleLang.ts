import type { TFunction } from "i18next";

/**
 * Язык просмотра тайтла. Он не меняет язык метаданных: это выбор того,
 * какие переводы показывать в главах, читалке и настройках.
 */

type LangSource = { lang: string; enabled: boolean };

/** Как в админке: русские источники первыми, англоязычные вторыми. */
const BASE_LANGS = ["ru", "en"];

export function sourceLanguages(
  sources: readonly LangSource[] | undefined,
): string[] {
  const languages = new Set<string>();
  for (const source of sources ?? []) {
    // Выключенный источник не может отдать главу: его язык не предлагаем.
    if (source.enabled && source.lang !== "") languages.add(source.lang);
  }
  return [...languages].sort((left, right) => {
    const leftRank = BASE_LANGS.indexOf(left);
    const rightRank = BASE_LANGS.indexOf(right);
    if (leftRank !== rightRank) {
      if (leftRank === -1) return 1;
      if (rightRank === -1) return -1;
      return leftRank - rightRank;
    }
    return left.localeCompare(right);
  });
}

/**
 * Возвращает выбранный язык или `null`, когда переключать нечего: у тайтла
 * меньше двух языков источников, и страница должна вести себя как раньше.
 */
export function resolveTitleLang({
  sources,
  requested,
  remembered,
  titleLang,
}: {
  sources: readonly LangSource[] | undefined;
  /** Язык из URL: он важнее сохранённого выбора. */
  requested: string | null;
  /** Последний выбор в этом браузере. */
  remembered: string | null;
  /** Язык самого тайтла из метаданных — язык по умолчанию. */
  titleLang: string | null | undefined;
}): string | null {
  const languages = sourceLanguages(sources);
  if (languages.length < 2) return null;

  for (const candidate of [requested, remembered, titleLang]) {
    if (
      candidate !== null &&
      candidate !== undefined &&
      languages.includes(candidate)
    ) {
      return candidate;
    }
  }
  return languages[0];
}

export function languageLabel(lang: string, t: TFunction): string {
  if (lang === "ru") return t("title.langRu");
  if (lang === "en") return t("title.langEn");
  return lang.toUpperCase();
}
