import type { LibraryState } from "@/api";
import type { LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { ReaderBookmarks } from "./ReaderBookmarks";
import { ReaderSettings } from "./ReaderSettings";

type LibraryActions = {
  library: LibraryState | null;
  pending: boolean;
  errorText: string | null;
  onAdd: () => void;
  onStatus: (status: LibraryStatus) => void;
  onToggleFavorite: () => void;
};

type ReaderControlsProps = LibraryActions & {
  /** Текущая страница, 1-based. */
  page: number;
  total: number;
  hidden: boolean;
};

/** Счётчик страниц и настройки справа; на телефоне живут в нижней панели. */
export function ReaderSideControls({
  page,
  total,
}: Pick<ReaderControlsProps, "page" | "total">) {
  const t = useT();

  return (
    <div className="absolute top-1/2 right-3 z-20 hidden -translate-y-1/2 flex-col items-center gap-3 md:flex">
      {total > 0 && (
        <span className="rounded-full bg-background/80 px-2.5 py-1 text-xs tabular-nums text-muted-foreground shadow-sm ring-1 ring-border backdrop-blur">
          {t("reader.pageOf", { page, total })}
        </span>
      )}
      <ReaderSettings page={page} total={total} />
    </div>
  );
}

/** Нижняя панель телефона: страница, настройки и закладки. */
export function ReaderMobileControls({
  page,
  total,
  hidden,
  library,
  pending,
  errorText,
  onAdd,
  onStatus,
  onToggleFavorite,
}: ReaderControlsProps) {
  const t = useT();

  return (
    <div
      className={cn(
        "absolute inset-x-0 bottom-0 z-30 flex flex-col items-center gap-2 px-4 pb-[calc(1rem+env(safe-area-inset-bottom))] transition-[transform,opacity] duration-200 md:hidden",
        hidden && "pointer-events-none translate-y-full opacity-0",
      )}
    >
      {total > 0 && (
        <span className="rounded-full bg-background/80 px-2.5 py-1 text-xs tabular-nums shadow-sm ring-1 ring-border backdrop-blur">
          {t("reader.pageOf", { page, total })}
        </span>
      )}
      <div className="flex items-center gap-2">
        <ReaderSettings page={page} total={total} />
        <ReaderBookmarks
          library={library}
          pending={pending}
          errorText={errorText}
          onAdd={onAdd}
          onStatus={onStatus}
          onToggleFavorite={onToggleFavorite}
        />
      </div>
    </div>
  );
}
