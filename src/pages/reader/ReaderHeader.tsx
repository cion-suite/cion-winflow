import { Link } from "react-router";
import {
  ArrowLeftIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  HeartIcon,
} from "lucide-react";

import { useT } from "@/lib/i18n";
import { readerPath, titlePath } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import { ReaderUserMenu } from "./ReaderUserMenu";

type ReaderHeaderProps = {
  titleId: number;
  titleName: string;
  chapterLabel: string;
  previousChapterKey: string | null;
  nextChapterKey: string | null;
  favorite: boolean;
  favoritePending: boolean;
  favoriteError: string | null;
  hidden: boolean;
  onToggleFavorite: () => void;
};

/** Верхняя панель: на узком экране только навигация, на широком — ещё тайтл и «Любимое». */
export function ReaderHeader({
  titleId,
  titleName,
  chapterLabel,
  previousChapterKey,
  nextChapterKey,
  favorite,
  favoritePending,
  favoriteError,
  hidden,
  onToggleFavorite,
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
        <div className="flex min-w-0 items-center gap-1">
          <Link
            to="/"
            aria-label={t("library.title")}
            className="shrink-0 rounded-full outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          >
            <img
              src="/logo.png"
              alt=""
              width={28}
              height={28}
              className="size-7 rounded-full"
            />
          </Link>
          <Link
            to={titlePath(titleId)}
            aria-label={t("reader.backToTitle")}
            className={buttonVariants({ variant: "ghost", size: "icon-sm" })}
          >
            <ArrowLeftIcon aria-hidden />
          </Link>
          <span className="hidden min-w-0 truncate text-sm font-medium md:block">
            {titleName}
          </span>
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

        <div className="relative flex min-w-0 items-center justify-end gap-1">
          <Button
            type="button"
            variant={favorite ? "default" : "secondary"}
            size="lg"
            className="hidden md:inline-flex"
            aria-pressed={favorite}
            disabled={favoritePending}
            onClick={onToggleFavorite}
          >
            <HeartIcon
              data-icon="inline-start"
              className={favorite ? "fill-current" : undefined}
            />
            {t("library.favorite")}
          </Button>
          <ReaderUserMenu />
          {favoriteError && (
            <p
              role="alert"
              className="absolute top-full right-0 mt-1 max-w-56 rounded-md bg-destructive/10 px-2 py-1 text-xs text-destructive"
            >
              {favoriteError}
            </p>
          )}
        </div>
      </div>
    </header>
  );
}
