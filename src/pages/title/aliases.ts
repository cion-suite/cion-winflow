import type { TFunction } from "i18next";

import type {
  AliasRange,
  AliasRangeInput,
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

export type RangePreviewPair = {
  alias: AliasVariant;
  canonical: AliasVariant;
};

export type RangePreview = {
  pairs: RangePreviewPair[];
  linked: number;
  pending: number;
};

function variantOrder(variant: AliasVariant): [number, number, string] {
  return [
    variant.volume ?? 0,
    variant.numberMilli ?? Number.MAX_SAFE_INTEGER,
    variant.chapterKey,
  ];
}

function compareOrder(
  left: [number, number, string],
  right: [number, number, string],
): number {
  return (
    left[0] - right[0] ||
    left[1] - right[1] ||
    left[2].localeCompare(right[2])
  );
}

/**
 * Клиентский предпросмотр связки. Сервер всё равно пересчитывает пары
 * по своим данным, но в диалоге нужно сразу показать, что получится.
 */
export function previewRange(
  aliasVariants: readonly AliasVariant[],
  canonicalVariants: readonly AliasVariant[],
  aliasAnchorKey: string | null,
  canonicalAnchorKey: string | null,
): RangePreview {
  const aliasAnchor = aliasVariants.find(
    (variant) => variant.chapterKey === aliasAnchorKey,
  );
  const canonicalAnchor = canonicalVariants.find(
    (variant) => variant.chapterKey === canonicalAnchorKey,
  );
  if (aliasAnchor === undefined || canonicalAnchor === undefined) {
    return { pairs: [], linked: 0, pending: 0 };
  }

  const volumeDelta =
    aliasAnchor.volume !== null && canonicalAnchor.volume !== null
      ? canonicalAnchor.volume - aliasAnchor.volume
      : null;
  const numberDelta =
    aliasAnchor.numberMilli !== null && canonicalAnchor.numberMilli !== null
      ? canonicalAnchor.numberMilli - aliasAnchor.numberMilli
      : 0;
  const anchorOrder = variantOrder(aliasAnchor);

  const pairs: RangePreviewPair[] = [];
  let pending = 0;
  for (const alias of aliasVariants) {
    if (compareOrder(variantOrder(alias), anchorOrder) < 0) continue;
    // Связка одного типа не покрывает special/extra: они не «ожидают»,
    // а просто живут отдельно.
    if (alias.chapterKind !== aliasAnchor.chapterKind) continue;
    if (alias.numberMilli === null || alias.numberText === null) {
      pending += 1;
      continue;
    }

    const targetVolume =
      volumeDelta === null
        ? alias.volume
        : alias.volume === null
          ? null
          : alias.volume + volumeDelta;
    const targetMilli = alias.numberMilli + numberDelta;
    const matches = canonicalVariants.filter((candidate) => {
      if (candidate.chapterKind !== alias.chapterKind) return false;
      if (volumeDelta === null) {
        if (candidate.volume !== null) return false;
      } else if (candidate.volume !== targetVolume) {
        return false;
      }
      return numberDelta === 0
        ? candidate.numberText === alias.numberText
        : candidate.numberMilli === targetMilli;
    });

    if (matches.length === 1 && matches[0].chapterKey !== alias.chapterKey) {
      pairs.push({ alias, canonical: matches[0] });
    } else {
      pending += 1;
    }
  }

  return { pairs, linked: pairs.length, pending };
}

export type RangeProposal = {
  id: string;
  a: SourceVariants;
  b: SourceVariants;
  anchorA: AliasVariant;
  anchorB: AliasVariant;
  count: number;
  pairs: Array<{ a: AliasVariant; b: AliasVariant }>;
  suggestedCanonical: "a" | "b";
};

export type RangeNote = {
  id: string;
  kind: "only_in" | "duplicate";
  sourceName: string;
  numberText: string;
};

type NumberPair = {
  a: AliasVariant;
  b: AliasVariant;
};

function matchableNumber(variant: AliasVariant): string | null {
  const text = variant.numberText?.trim() ?? "";
  if (text === "" || text.includes("-") || variant.numberMilli === null) {
    return null;
  }
  return text;
}

function numberBuckets(
  variants: readonly AliasVariant[],
): Map<string, AliasVariant[]> {
  const buckets = new Map<string, AliasVariant[]>();
  for (const variant of variants) {
    const number = matchableNumber(variant);
    if (number === null) continue;
    const tag = `${variant.chapterKind}:${number}`;
    const list = buckets.get(tag) ?? [];
    list.push(variant);
    buckets.set(tag, list);
  }
  return buckets;
}

/**
 * Предложения диапазонов: пары с одинаковым номером и разными ключами,
 * у которых подряд идёт одно и то же расхождение по тому. Только
 * предложения; решение и сторона канона остаются за администратором.
 */
export function proposeRanges(
  chapters: readonly TitleChapter[],
  sources: readonly TitleSource[],
  ranges: readonly AliasRange[],
  preferredSourceId: number | null,
): { proposals: RangeProposal[]; notes: RangeNote[] } {
  const variants = collectVariants(chapters, sources, preferredSourceId);
  if (variants.length < 2) {
    return { proposals: [], notes: [] };
  }

  const preferred =
    variants.find((item) => item.source.source_id === preferredSourceId) ??
    variants[0];
  const others = variants.filter(
    (item) => item.source.source_id !== preferred.source.source_id,
  );
  const sourcePairs: Array<[SourceVariants, SourceVariants]> = others.map(
    (other) => [preferred, other],
  );

  const proposals: RangeProposal[] = [];
  const notes: RangeNote[] = [];
  const seenNotes = new Set<string>();

  for (const [left, right] of sourcePairs) {
    const leftBuckets = numberBuckets(left.variants);
    const rightBuckets = numberBuckets(right.variants);

    const matched: NumberPair[] = [];
    for (const [tag, leftVariants] of leftBuckets) {
      const rightVariants = rightBuckets.get(tag) ?? [];
      if (leftVariants.length !== 1 || rightVariants.length !== 1) continue;
      const [a] = leftVariants;
      const [b] = rightVariants;
      if (a.chapterKey === b.chapterKey) continue;
      if (a.volume === null || b.volume === null) continue;
      matched.push({ a, b });
    }
    matched.sort((first, second) =>
      compareOrder(variantOrder(first.a), variantOrder(second.a)),
    );

    let run: NumberPair[] = [];
    const flush = (): void => {
      if (run.length < 2) {
        run = [];
        return;
      }
      const first = run[0];
      const covered = ranges.some(
        (range) =>
          (range.source_entry_id === left.source.source_entry_id &&
            range.anchor.chapter_key === first.a.chapterKey) ||
          (range.source_entry_id === right.source.source_entry_id &&
            range.anchor.chapter_key === first.b.chapterKey),
      );
      if (!covered) {
        const bHigher = run.filter(
          (pair) => (pair.b.volume ?? 0) > (pair.a.volume ?? 0),
        ).length;
        const aHigher = run.filter(
          (pair) => (pair.a.volume ?? 0) > (pair.b.volume ?? 0),
        ).length;
        proposals.push({
          id: `${left.source.source_entry_id}:${first.a.chapterKey}:${right.source.source_entry_id}:${first.b.chapterKey}`,
          a: left,
          b: right,
          anchorA: first.a,
          anchorB: first.b,
          count: run.length,
          pairs: run,
          suggestedCanonical: aHigher > bHigher ? "a" : "b",
        });
      }
      run = [];
    };

    for (const pair of matched) {
      const volumeDelta = (pair.b.volume ?? 0) - (pair.a.volume ?? 0);
      const previous = run[run.length - 1];
      const previousDelta = previous
        ? (previous.b.volume ?? 0) - (previous.a.volume ?? 0)
        : null;
      if (previous !== undefined && previousDelta !== volumeDelta) {
        flush();
      }
      run.push(pair);
    }
    flush();

    // Замечания: главы, которые есть только на одной стороне, и повторы
    // номера. Это не связки — автоматически их не трогаем.
    for (const [tag, leftVariants] of leftBuckets) {
      if (leftVariants.length === 1 && !rightBuckets.has(tag)) {
        const variant = leftVariants[0];
        const key = `only:${left.source.source_id}:${tag}`;
        if (!seenNotes.has(key)) {
          seenNotes.add(key);
          notes.push({
            id: key,
            kind: "only_in",
            sourceName: left.source.name,
            numberText: variant.numberText ?? "",
          });
        }
      }
    }
    for (const [tag, rightVariants] of rightBuckets) {
      if (rightVariants.length === 1 && !leftBuckets.has(tag)) {
        const variant = rightVariants[0];
        const key = `only:${right.source.source_id}:${tag}`;
        if (!seenNotes.has(key)) {
          seenNotes.add(key);
          notes.push({
            id: key,
            kind: "only_in",
            sourceName: right.source.name,
            numberText: variant.numberText ?? "",
          });
        }
      }
    }
  }

  for (const source of variants) {
    const buckets = numberBuckets(source.variants);
    for (const [tag, list] of buckets) {
      if (list.length <= 1) continue;
      const key = `dup:${source.source.source_id}:${tag}`;
      if (seenNotes.has(key)) continue;
      seenNotes.add(key);
      notes.push({
        id: key,
        kind: "duplicate",
        sourceName: source.source.name,
        numberText: list[0].numberText ?? "",
      });
    }
  }

  return { proposals, notes: notes.slice(0, 12) };
}

export function rangeInputForProposal(
  proposal: RangeProposal,
  canonical: "a" | "b",
): AliasRangeInput {
  const linked = canonical === "a" ? proposal.b : proposal.a;
  const linkedAnchor = canonical === "a" ? proposal.anchorB : proposal.anchorA;
  const canonicalAnchor =
    canonical === "a" ? proposal.anchorA : proposal.anchorB;
  return {
    source_entry_id: linked.source.source_entry_id,
    anchor_chapter_id: linkedAnchor.chapterId,
    canonical_chapter_id: canonicalAnchor.chapterId,
  };
}

export function findCanonicalSide(
  sourceVariants: readonly SourceVariants[],
  canonicalKey: string,
  canonicalChapterId: number | null,
): SourceVariants | undefined {
  return (
    sourceVariants.find((item) =>
      item.variants.some(
        (variant) =>
          variant.chapterKey === canonicalKey &&
          (canonicalChapterId === null ||
            variant.chapterId === canonicalChapterId),
      ),
    ) ??
    sourceVariants.find((item) =>
      item.variants.some((variant) => variant.chapterKey === canonicalKey),
    )
  );
}
