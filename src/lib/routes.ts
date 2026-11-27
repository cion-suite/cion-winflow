/** Пути экранов вне меню: chapter_key содержит двоеточия и точки, его нужно кодировать. */
const segment = (value: string | number): string => encodeURIComponent(String(value));

/** Язык просмотра передаётся query-параметром: он переживает перезагрузку. */
const langQuery = (lang: string | null | undefined): string =>
  lang === null || lang === undefined || lang === ""
    ? ""
    : `?lang=${segment(lang)}`;

export function titlePath(
  id: string | number,
  lang?: string | null,
): string {
  return `/title/${segment(id)}${langQuery(lang)}`;
}

/** Публичный профиль по uid: ссылки на чужие профили собирай здесь. */
export function profilePath(id: string | number): string {
  return `/profile/${segment(id)}`;
}

export function readerPath(
  titleId: string | number,
  chapterKey: string,
  lang?: string | null,
): string {
  return `/reader/${segment(titleId)}/${segment(chapterKey)}${langQuery(lang)}`;
}

export function watchPath(titleId: string | number, chapterKey: string): string {
  return `/watch/${segment(titleId)}/${segment(chapterKey)}`;
}
