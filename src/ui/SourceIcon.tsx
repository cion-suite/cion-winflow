import { GlobeIcon } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * Логотипы лежат в `public/sources/`: источники захардкожены как серверные
 * адаптеры, поэтому внешний favicon-сервис здесь не нужен и CSP не страдает.
 * Неизвестный код получает нейтральную иконку.
 */
const SOURCE_ICONS: Record<
  string,
  { src: string; monochrome?: boolean }
> = {
  remanga: { src: "/sources/remanga.png", monochrome: true },
  rumix: { src: "/sources/rumix.png" },
  mangamir: { src: "/sources/mangamir.png", monochrome: true },
};

export function SourceIcon({
  code,
  className,
}: {
  code: string;
  className?: string;
}) {
  const source = SOURCE_ICONS[code];

  if (source === undefined) {
    return (
      <GlobeIcon
        aria-hidden
        className={cn("size-5 text-muted-foreground", className)}
      />
    );
  }

  return (
    <img
      src={source.src}
      alt=""
      aria-hidden
      // Чёрные логотипы Remanga/Mangamir иначе исчезают на тёмной теме.
      className={cn(
        "size-5 object-contain",
        source.monochrome && "dark:invert",
        className,
      )}
    />
  );
}
