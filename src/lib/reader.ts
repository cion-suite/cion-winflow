/** Варианты ширины колонки страниц в читалке. `full` — во всю ширину окна. */
export const READER_WIDTHS = [
  "sm",
  "md",
  "lg",
  "xl",
  "2xl",
  "3xl",
  "4xl",
  "5xl",
  "6xl",
  "full",
] as const;

export type ReaderWidth = (typeof READER_WIDTHS)[number];

/** Прежняя колонка ридера — max-w-3xl. */
export const DEFAULT_READER_WIDTH: ReaderWidth = "3xl";

export function isReaderWidth(value: unknown): value is ReaderWidth {
  return (
    typeof value === "string" &&
    (READER_WIDTHS as readonly string[]).includes(value)
  );
}

/** Классы записаны литералами: Tailwind видит их при сканировании. */
export const READER_WIDTH_CLASS: Record<ReaderWidth, string> = {
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-lg",
  xl: "max-w-xl",
  "2xl": "max-w-2xl",
  "3xl": "max-w-3xl",
  "4xl": "max-w-4xl",
  "5xl": "max-w-5xl",
  "6xl": "max-w-6xl",
  full: "max-w-none",
};
