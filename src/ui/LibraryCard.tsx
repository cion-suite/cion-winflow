import { HeartIcon, StarIcon } from "lucide-react";
import { Link } from "react-router";

import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { CoverImage } from "@/ui/CoverImage";

/**
 * Достаточная часть записи библиотеки: экраны передают свой `LibraryItem`,
 * а обложку считают сами — `ui` не знает про API.
 */
export type LibraryCardItem = {
  title_id: number;
  status: string;
  score: number | null;
  custom_tab_id: number | null;
  read_count: number;
  chapter_count: number;
  title: {
    name: string;
    kind: string;
    year: number | null;
  };
};

type LibraryCardProps = {
  item: LibraryCardItem;
  coverUrl: string | null;
};

/** Карточка тайтла в библиотеке: обложка, тип, прогресс и оценка. */
export function LibraryCard({ item, coverUrl }: LibraryCardProps) {
  const t = useT();
  const hasProgress = item.chapter_count > 0;

  return (
    <Link
      to={titlePath(item.title_id)}
      className="group flex min-w-0 flex-col gap-2 rounded-xl outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <div className="relative overflow-hidden rounded-xl ring-1 ring-foreground/10">
        <CoverImage
          src={coverUrl}
          alt={item.title.name}
          className="aspect-2/3 w-full transition-transform duration-200 group-hover:scale-[1.02]"
        />
        {item.status === "favorite" && item.custom_tab_id === null && (
          <span className="absolute top-2 right-2 grid size-7 place-items-center rounded-full bg-background/85 text-primary shadow-sm">
            <HeartIcon className="size-4 fill-current" aria-hidden />
            <span className="sr-only">{t("library.statuses.favorite")}</span>
          </span>
        )}
        {item.score !== null && (
          <span className="absolute right-2 bottom-2 inline-flex items-center gap-1 rounded-full bg-background/85 px-2 py-1 text-xs font-semibold text-foreground shadow-sm">
            <StarIcon className="size-3.5 fill-current" aria-hidden />
            <span className="tabular-nums">{item.score}</span>
            <span className="sr-only">{t("library.score")}</span>
          </span>
        )}
      </div>

      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="truncate">
          {t(`catalog.types.${item.title.kind}`)}
          {item.title.year !== null ? ` ${item.title.year}` : ""}
        </span>
        {hasProgress && (
          <span className="inline-flex shrink-0 items-center gap-1 tabular-nums">
            {Math.min(item.read_count, item.chapter_count)}/{item.chapter_count}
          </span>
        )}
      </div>

      <span className="line-clamp-2 text-sm font-medium">{item.title.name}</span>
    </Link>
  );
}
