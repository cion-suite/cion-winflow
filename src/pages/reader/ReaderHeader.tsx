import { Link } from "react-router";
import {
  ArrowLeftIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
} from "lucide-react";

import type { LibraryState, LibraryTab } from "@/api";
import type { LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { readerPath, titlePath } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { buttonVariants } from "@/ui/shadcn/button";
import { ReaderBookmarks } from "./ReaderBookmarks";
import { ReaderUserMenu } from "./ReaderUserMenu";

type ReaderHeaderProps = {
  titleId: number;
  titleName: string;
  chapterLabel: string;
  previousChapterKey: string | null;
  nextChapterKey: string | null;
  library: LibraryState | null;
  tabs: LibraryTab[];
  pending: boolean;
  errorText: string | null;
  hidden: boolean;
  onAdd: () => void;
  onStatus: (status: LibraryStatus) => void;
  onCustomStatus: (tabId: number) => void;
};

/**
 * Верхняя панель: на узком экране только навигация, на широком — ещё
 * название тайтла одной ссылкой со стрелкой и выбор своих закладок.
 */
export function ReaderHeader({
  titleId,
  titleName,
  chapterLabel,
  previousChapterKey,
  nextChapterKey,
  library,
  tabs,
  pending,
  errorText,
  hidden,
  onAdd,
  onStatus,
  onCustomStatus,
}: ReaderHeaderProps) {
  const t = useT();

  return (
    <header
      className={cn(
        "absolute inset-x-0 top-0 z-30 border-b bg-background/90 backdrop-blur transition-[transform,opacity] duration-200",
        hidden && "pointer-events-none -translate-y-full opacity-0",
      )}
    >
      <div className="grid h-14 grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2 px-2 sm:px-4">
        <div className="flex min-w-0 items-center">
          <Link
            to={titlePath(titleId)}
            aria-label={t("reader.backToTitle")}
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              "min-w-0 gap-1.5 px-2",
            )}
          >
            <ArrowLeftIcon aria-hidden />
            <span className="hidden min-w-0 truncate text-sm font-medium md:block">
              {titleName}
            </span>
          </Link>
        </div>

        <div className="flex items-center gap-0.5 rounded-full border bg-background/70 p-0.5">
          {previousChapterKey ? (
            <Link
              to={readerPath(titleId, previousChapterKey)}
              aria-label={t("reader.prevChapter")}
              className={buttonVariants({ variant: "ghost", size: "icon-sm" })}
            >
              <ChevronLeftIcon aria-hidden />
            </Link>
          ) : (
            <span
              aria-hidden
              className={cn(
                buttonVariants({ variant: "ghost", size: "icon-sm" }),
                "pointer-events-none opacity-40",
              )}
            >
              <ChevronLeftIcon />
            </span>
          )}
          <span className="min-w-12 px-1 text-center text-sm font-medium tabular-nums">
            {chapterLabel}
          </span>
          {nextChapterKey ? (
            <Link
              to={readerPath(titleId, nextChapterKey)}
              aria-label={t("reader.nextChapter")}
              className={buttonVariants({ variant: "ghost", size: "icon-sm" })}
            >
              <ChevronRightIcon aria-hidden />
            </Link>
          ) : (
            <span
              aria-hidden
              className={cn(
                buttonVariants({ variant: "ghost", size: "icon-sm" }),
                "pointer-events-none opacity-40",
              )}
            >
              <ChevronRightIcon />
            </span>
          )}
        </div>

        <div className="flex min-w-0 items-center justify-end gap-1">
          <div className="hidden md:block">
            <ReaderBookmarks
              variant="wide"
              library={library}
              tabs={tabs}
              pending={pending}
              errorText={errorText}
              onAdd={onAdd}
              onStatus={onStatus}
              onCustomStatus={onCustomStatus}
            />
          </div>
          <ReaderUserMenu />
        </div>
      </div>
    </header>
  );
}
