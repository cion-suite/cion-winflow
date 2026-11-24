/** Пути экранов вне меню: chapter_key содержит двоеточия и точки, его нужно кодировать. */
const segment = (value: string | number): string => encodeURIComponent(String(value));

export function titlePath(id: string | number): string {
  return `/title/${segment(id)}`;
}

export function readerPath(titleId: string | number, chapterKey: string): string {
  return `/reader/${segment(titleId)}/${segment(chapterKey)}`;
}

export function watchPath(titleId: string | number, chapterKey: string): string {
  return `/watch/${segment(titleId)}/${segment(chapterKey)}`;
}
