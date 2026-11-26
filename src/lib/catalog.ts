/** Значения, общие для API и форм: строки-коды, которые принимает сервер. */

export const MEDIA_KINDS = ["manga", "anime", "book"] as const;
export type MediaKind = (typeof MEDIA_KINDS)[number];

export const TITLE_STATUSES = ["ongoing", "completed", "hiatus", "cancelled"] as const;
export type TitleStatus = (typeof TITLE_STATUSES)[number];

export const AVAILABILITIES = ["unknown", "available", "locked", "missing"] as const;
export type Availability = (typeof AVAILABILITIES)[number];

export const CONTENT_KINDS = ["images", "video", "text"] as const;
export type ContentKind = (typeof CONTENT_KINDS)[number];

/** Размеры обложки: low — поиск и добавление, mid — библиотека, high — тайтл. */
export const COVER_SIZES = ["low", "mid", "high"] as const;
export type CoverSize = (typeof COVER_SIZES)[number];

/** Порядок вкладок и выпадающих списков. «Любимое» — обычный статус. */
export const LIBRARY_STATUSES = [
  "in_progress",
  "planned",
  "favorite",
  "completed",
  "dropped",
  "on_hold",
  "not_interested",
] as const;
export type LibraryStatus = (typeof LIBRARY_STATUSES)[number];

/** Своя закладка в объединённом выборе статуса. */
export function customStatusValue(tabId: number): string {
  return `custom:${tabId}`;
}

/** null — встроенный статус, число — id своей закладки. */
export function customTabIdFromValue(value: string): number | null {
  if (!value.startsWith("custom:")) return null;
  const id = Number(value.slice("custom:".length));
  return Number.isInteger(id) && id > 0 ? id : null;
}

/** Поля формы тайтла: списки и числа вводятся строками (через запятую). */
export type TitleFormValues = {
  name: string;
  kind: MediaKind;
  altNames: string;
  authors: string;
  genres: string;
  description: string;
  status: "" | TitleStatus;
  year: string;
  lang: string;
  rating: string;
};

export function emptyTitleFormValues(): TitleFormValues {
  return {
    name: "",
    kind: "manga",
    altNames: "",
    authors: "",
    genres: "",
    description: "",
    status: "",
    year: "",
    lang: "",
    rating: "",
  };
}

/** "a, b,,c" -> ["a", "b", "c"]: пустые элементы не доходят до сервера. */
export function splitList(value: string): string[] {
  return value
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

export function joinList(values: readonly string[]): string {
  return values.join(", ");
}
