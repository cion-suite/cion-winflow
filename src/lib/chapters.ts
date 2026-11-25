/** Глава в списке: структурный тип, чтобы lib не зависел от api. */
export type ChapterShape = {
  chapter_key: string;
  volume: number | null;
  number_milli: number | null;
  availability: string;
  variants: readonly { source_enabled: boolean }[];
};

/**
 * Недоступные главы не показываем ни на странице тайтла, ни в переходах
 * ридера: у них нет ни одного рабочего варианта.
 */
export function isAvailableChapter(chapter: ChapterShape): boolean {
  if (chapter.availability === "locked" || chapter.availability === "missing") {
    return false;
  }
  return chapter.variants.some((variant) => variant.source_enabled);
}

/** Дробные номера и спецвыпуски: число, затем ключ; специфика всегда в конце. */
export function sortChapters<T extends ChapterShape>(
  chapters: readonly T[],
  ascending: boolean,
): T[] {
  return [...chapters].sort((left, right) => {
    const leftVolume = left.volume ?? 0;
    const rightVolume = right.volume ?? 0;
    if (leftVolume !== rightVolume) {
      return ascending ? leftVolume - rightVolume : rightVolume - leftVolume;
    }

    const leftNumber = left.number_milli;
    const rightNumber = right.number_milli;
    if (leftNumber === null && rightNumber === null) {
      return left.chapter_key.localeCompare(right.chapter_key);
    }
    if (leftNumber === null) return 1;
    if (rightNumber === null) return -1;
    if (leftNumber !== rightNumber) {
      return ascending ? leftNumber - rightNumber : rightNumber - leftNumber;
    }
    return left.chapter_key.localeCompare(right.chapter_key);
  });
}
