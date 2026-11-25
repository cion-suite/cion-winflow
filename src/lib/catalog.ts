/** Значения, общие для API и форм: строки-коды, которые принимает сервер. */

export const MEDIA_KINDS = ["manga", "anime", "book"] as const;
export type MediaKind = (typeof MEDIA_KINDS)[number];

export const TITLE_STATUSES = ["ongoing", "completed", "hiatus", "cancelled"] as const;
export type TitleStatus = (typeof TITLE_STATUSES)[number];

export const AVAILABILITIES = ["unknown", "available", "locked", "missing"] as const;
export type Availability = (typeof AVAILABILITIES)[number];

export const CONTENT_KINDS = ["images", "video", "text"] as const;
export type ContentKind = (typeof CONTENT_KINDS)[number];

export const LIBRARY_STATUSES = [
  "planned",
  "in_progress",
  "completed",
  "on_hold",
  "dropped",
  "not_interested",
] as const;
export type LibraryStatus = (typeof LIBRARY_STATUSES)[number];

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
