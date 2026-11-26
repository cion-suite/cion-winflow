import type { TFunction } from "i18next";

import type {
  ChapterAlias,
  TitleChapter,
  TitleSource,
} from "@/api";

/** Вариант главы в плоскости одного источника: нужен для ручной связки. */
export type AliasVariant = {
  chapterId: number;
  chapterKey: string;
  volume: number | null;
  numberText: string | null;
  numberMilli: number | null;
  chapterKind: string;
  sourceId: number;
  sourceCode: string;
  sourceName: string;
  sourceEnabled: boolean;
  availability: string;
  /** Выбранный сервером вариант: именно он откроется у этой главы. */
  selected: boolean;
};

export type SourceVariants = {
  source: TitleSource;
  variants: AliasVariant[];
};

export type AliasPair = {
  numberMilli: number;
  left: AliasVariant;
  right: AliasVariant;
};

export type AliasGroup = {
  canonicalKey: string;
  aliasKeys: Set<string>;
  /** Группа главы с канонической нумерацией; null — остались только alias. */
  chapter: TitleChapter | null;
};

export type CanonicalSide = "left" | "right";

/**
 * Порядок источников, который использует сервер: сначала включённые,
 * затем ранг приоритета, preferred и primary. Выше — важнее.
 */
export function sourceOrder(
  sources: readonly TitleSource[],
  preferredSourceId: number | null,
): TitleSource[] {
  return [...sources].sort((left, right) => {
    const leftRank = left.priority_rank ?? Number.MAX_SAFE_INTEGER;
    const rightRank = right.priority_rank ?? Number.MAX_SAFE_INTEGER;
    return (
      Number(!left.enabled) - Number(!right.enabled) ||
      leftRank - rightRank ||
      Number(left.source_id !== preferredSourceId) -
        Number(right.source_id !== preferredSourceId) ||
      Number(!left.is_primary) - Number(!right.is_primary) ||
      right.available_count - left.available_count ||
      left.name.localeCompare(right.name)
    );
  });
}

/** Главы, сгруппированные по источникам: списки для выбора в диалогах. */
export function collectVariants(
  chapters: readonly TitleChapter[],
  sources: readonly TitleSource[],
  preferredSourceId: number | null,
): SourceVariants[] {
  // Несколько групп перевода могут отдать одну главу с одним ключом.
  // Для alias это одна запись, поэтому внутри источника ключ уникален.
  const bySource = new Map<number, Map<string, AliasVariant>>();
  for (const chapter of chapters) {
    for (const variant of chapter.variants) {
      const variants = bySource.get(variant.source_id) ?? new Map();
      const existing = variants.get(variant.chapter_key);
      if (existing !== undefined) {
        existing.selected =
          existing.selected ||
          chapter.selected_chapter_id === variant.chapter_id;
        continue;
      }
      variants.set(variant.chapter_key, {
        chapterId: variant.chapter_id,
        chapterKey: variant.chapter_key,
        volume: variant.volume,
        numberText: variant.number_text,
        numberMilli: variant.number_milli,
        chapterKind: variant.chapter_kind,
        sourceId: variant.source_id,
        sourceCode: variant.source_code,
        sourceName: variant.source_name,
        sourceEnabled: variant.source_enabled,
        availability: variant.availability,
        selected: chapter.selected_chapter_id === variant.chapter_id,
      });
      bySource.set(variant.source_id, variants);
    }
  }

  return sourceOrder(sources, preferredSourceId)
    .filter((source) => bySource.has(source.source_id))
    .map((source) => ({
      source,
      variants: sortVariants([...(bySource.get(source.source_id)?.values() ?? [])]),
    }));
}

function sortVariants(variants: AliasVariant[]): AliasVariant[] {
  return [...variants].sort(
    (left, right) =>
      (left.volume ?? 0) - (right.volume ?? 0) ||
      (left.numberMilli ?? Number.MAX_SAFE_INTEGER) -
        (right.numberMilli ?? Number.MAX_SAFE_INTEGER) ||
      left.chapterKey.localeCompare(right.chapterKey),
  );
}

export function aliasLookup(aliases: readonly ChapterAlias[]): Map<string, string> {
  return new Map(aliases.map((alias) => [alias.alias_key, alias.canonical_key]));
}

function resolveKey(lookup: Map<string, string>, key: string): string {
  return lookup.get(key) ?? key;
}

/**
 * Номер для сопоставления. `number_milli` у диапазона («10-11») равен
 * началу диапазона, поэтому сравнивать только по нему нельзя: одиночная
 * «10» и релиз «10-11» — разные главы. Сравниваем нормализованный текст,
 * а диапазоны не предлагаем вовсе.
 */
function matchNumber(variant: AliasVariant): string | null {
  const text = variant.numberText?.trim() ?? "";
  if (text === "" || text.includes("-")) return null;
  return text;
}

/**
 * Главы с одинаковым номером и видом, но разными ключами. Ключи, которые
 * уже участвуют в alias, не предлагаем: связку сначала надо удалить,
 * иначе auto-match молча перезаписал бы решение администратора.
 */
export function matchByNumber(
  left: readonly AliasVariant[],
  right: readonly AliasVariant[],
  aliases: readonly ChapterAlias[],
): AliasPair[] {
  const aliasKeys = new Set(aliases.map((alias) => alias.alias_key));
  const linkedKeys = new Set([
    ...aliasKeys,
    ...aliases.map((alias) => alias.canonical_key),
  ]);
  const lookup = aliasLookup(aliases);

  // Группируем по номеру и виду. Пара неоднозначна, если в бакете больше
  // одной главы: номер повторяется по томам, и угадывать том нельзя.
  // Alias-ключ не может стать ни alias, ни canonical; готовый canonical
  // можно использовать снова — несколько alias на одну главу допустимы.
  const collect = (
    variants: readonly AliasVariant[],
    blocked: ReadonlySet<string>,
  ) => {
    const buckets = new Map<string, AliasVariant[]>();
    for (const variant of variants) {
      const number = matchNumber(variant);
      if (variant.numberMilli === null || number === null) continue;
      if (blocked.has(variant.chapterKey)) continue;
      const tag = `${variant.chapterKind}:${number}`;
      const list = buckets.get(tag) ?? [];
      list.push(variant);
      buckets.set(tag, list);
    }
    return buckets;
  };

  const leftBuckets = collect(left, linkedKeys);
  const rightBuckets = collect(right, aliasKeys);

  const pairs: AliasPair[] = [];
  for (const [tag, leftVariants] of leftBuckets) {
    const rightVariants = rightBuckets.get(tag) ?? [];
    if (rightVariants.length === 0) continue;

    // Тот же ключ на обеих сторонах уже живёт в одной группе — связка
    // не нужна, а поиск «другого» кандидата только испортил бы её.
    const alreadyAligned = leftVariants.some((leftVariant) =>
      rightVariants.some(
        (rightVariant) => rightVariant.chapterKey === leftVariant.chapterKey,
      ),
    );
    if (alreadyAligned) continue;

    // Повтор номера по томам с одной из сторон: не угадываем.
    if (leftVariants.length !== 1 || rightVariants.length !== 1) continue;

    const [leftVariant] = leftVariants;
    const [rightVariant] = rightVariants;
    if (
      resolveKey(lookup, leftVariant.chapterKey) ===
      resolveKey(lookup, rightVariant.chapterKey)
    ) {
      continue;
    }
    pairs.push({
      numberMilli: leftVariant.numberMilli ?? 0,
      left: leftVariant,
      right: rightVariant,
    });
  }

  return pairs.sort(
    (leftPair, rightPair) =>
      leftPair.numberMilli - rightPair.numberMilli ||
      leftPair.left.chapterKey.localeCompare(rightPair.left.chapterKey),
  );
}

/**
 * Том, который вырос, обычно и есть исправленный: источник мог забыть
 * сменить том, но редко придумывает лишний. Если тома расходятся в обе
 * стороны, подсказка неоднозначна — возвращаем null и требуем выбор,
 * иначе легко оставить неверную нумерацию. Это только подсказка.
 */
export function suggestCanonicalSide(
  pairs: readonly AliasPair[],
): CanonicalSide | null {
  let leftHigher = 0;
  let rightHigher = 0;
  for (const pair of pairs) {
    const leftVolume = pair.left.volume ?? 0;
    const rightVolume = pair.right.volume ?? 0;
    if (leftVolume > rightVolume) leftHigher += 1;
    if (rightVolume > leftVolume) rightHigher += 1;
  }
  if (leftHigher > 0 && rightHigher > 0) return null;
  return rightHigher > leftHigher ? "right" : "left";
}

export function buildAliasPairs(
  pairs: readonly AliasPair[],
  side: CanonicalSide,
): ChapterAlias[] {
  return pairs.map((pair) =>
    side === "left"
      ? { alias_key: pair.right.chapterKey, canonical_key: pair.left.chapterKey }
      : { alias_key: pair.left.chapterKey, canonical_key: pair.right.chapterKey },
  );
}

/** Alias сгруппированы по канонической главе и идут в порядке списка глав. */
export function groupAliases(
  aliases: readonly ChapterAlias[],
  chapters: readonly TitleChapter[],
): AliasGroup[] {
  const chapterByKey = new Map(chapters.map((chapter) => [chapter.chapter_key, chapter]));
  const position = new Map(
    chapters.map((chapter, index) => [chapter.chapter_key, index]),
  );
  const byCanonical = new Map<string, Set<string>>();
  for (const alias of aliases) {
    const keys = byCanonical.get(alias.canonical_key) ?? new Set<string>();
    keys.add(alias.alias_key);
    byCanonical.set(alias.canonical_key, keys);
  }

  return [...byCanonical.entries()]
    .map(([canonicalKey, aliasKeys]) => ({
      canonicalKey,
      aliasKeys,
      chapter: chapterByKey.get(canonicalKey) ?? null,
    }))
    .sort((left, right) => {
      const leftPosition = position.get(left.canonicalKey);
      const rightPosition = position.get(right.canonicalKey);
      if (leftPosition !== undefined || rightPosition !== undefined) {
        return (
          (leftPosition ?? Number.MAX_SAFE_INTEGER) -
            (rightPosition ?? Number.MAX_SAFE_INTEGER) ||
          left.canonicalKey.localeCompare(right.canonicalKey)
        );
      }
      return left.canonicalKey.localeCompare(right.canonicalKey);
    });
}

/** «Том 3 · Глава 151» — номер без тома, спецвыпуск без номера. */
export function formatChapterLabel(
  chapter: { volume: number | null; numberText: string | null },
  t: TFunction,
): string {
  const number =
    chapter.numberText === null || chapter.numberText === ""
      ? t("title.special")
      : t("title.chapterNumber", { number: chapter.numberText });
  return chapter.volume === null
    ? number
    : t("title.volumeChapter", { volume: chapter.volume, chapter: number });
}
