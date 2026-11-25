import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useParams } from "react-router";
import { ArrowLeftIcon, ChevronLeftIcon, ChevronRightIcon } from "lucide-react";

import {
  pageImageUrl,
  progressApi,
  useChapterContentQuery,
  useErrorText,
  useProgressQuery,
  useSaveProgressMutation,
  useTitleQuery,
  type ContentPage,
} from "@/api";
import { isAvailableChapter, sortChapters } from "@/lib/chapters";
import { useT } from "@/lib/i18n";
import { readerPath, titlePath } from "@/lib/routes";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import { Skeleton } from "@/ui/shadcn/skeleton";

const EMPTY_PAGES: ContentPage[] = [];
const SAVE_DEBOUNCE_MS = 800;
const AROUND = 2;
const AHEAD = 6;

/** Бинарный поиск страницы по накопленным высотам. */
function pageAt(offsets: number[], scrollTop: number): number {
  if (offsets.length < 2) return 0;
  let low = 0;
  let high = offsets.length - 2;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (offsets[middle] <= scrollTop) low = middle;
    else high = middle - 1;
  }
  return low;
}

/**
 * Смена главы должна пересоздавать состояние: иначе restoredRef,
 * позиция скролла и виртуальное окно останутся от прошлой главы.
 */
export function ReaderPage() {
  const { titleId: rawTitleId, chapterKey = null } = useParams();
  const parsed = Number(rawTitleId);
  const titleId = Number.isInteger(parsed) && parsed > 0 ? parsed : null;

  return (
    <ReaderView
      key={`${titleId ?? "invalid"}:${chapterKey ?? ""}`}
      titleId={titleId}
      chapterKey={chapterKey}
    />
  );
}

function ReaderView({
  titleId,
  chapterKey,
}: {
  titleId: number | null;
  chapterKey: string | null;
}) {
  const t = useT();

  const titleQuery = useTitleQuery(titleId);
  const contentQuery = useChapterContentQuery(titleId, chapterKey);
  const progressQuery = useProgressQuery(titleId);
  const save = useSaveProgressMutation(titleId ?? 0);
  const errorText = useErrorText(contentQuery.error);

  const content = contentQuery.data?.content;
  const pages = content?.kind === "images" ? content.pages : EMPTY_PAGES;
  // Переходы «назад/вперёд» не должны предлагать locked/missing главы
  const chapters = useMemo(
    () =>
      sortChapters(
        (titleQuery.data?.chapters ?? []).filter(isAvailableChapter),
        true,
      ),
    [titleQuery.data?.chapters],
  );

  const [width, setWidth] = useState(0);
  const [current, setCurrent] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);
  const offsetsRef = useRef<number[]>([]);
  const currentRef = useRef(0);
  const restoredRef = useRef(false);
  const rafRef = useRef<number | null>(null);

  const heights = useMemo(
    () =>
      pages.map((page) => {
        const ratio =
          page.width !== null && page.height !== null && page.width > 0
            ? page.height / page.width
            : 1.4;
        return Math.max(160, width * ratio);
      }),
    [pages, width],
  );

  const offsets = useMemo(() => {
    const values = new Array<number>(heights.length + 1);
    values[0] = 0;
    for (let index = 0; index < heights.length; index += 1) {
      values[index + 1] = values[index] + heights[index];
    }
    return values;
  }, [heights]);

  const totalHeight = offsets[offsets.length - 1] ?? 0;

  const start = Math.max(0, current - AROUND);
  const end = Math.min(pages.length, current + AHEAD);

  // Alias-URL резолвится сервером в canonical: навигация идёт по нему
  const canonicalKey = contentQuery.data?.chapter_key ?? chapterKey;
  const chapterIndex = canonicalKey
    ? chapters.findIndex((chapter) => chapter.chapter_key === canonicalKey)
    : -1;
  const previousChapter = chapterIndex > 0 ? chapters[chapterIndex - 1] : null;
  const nextChapter =
    chapterIndex >= 0 && chapterIndex < chapters.length - 1
      ? chapters[chapterIndex + 1]
      : null;

  const chapterTitle = useMemo(() => {
    if (!contentQuery.data) return t("reader.loading");
    const number = contentQuery.data.number_text
      ? `${t("title.chapter")} ${contentQuery.data.number_text}`
      : t("title.special");
    return contentQuery.data.name
      ? `${number} · ${contentQuery.data.name}`
      : number;
  }, [contentQuery.data, t]);

  // Ширина колонки: от неё зависят высоты страниц до загрузки картинок
  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return;

    const update = () => setWidth(element.clientWidth);
    update();

    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const handleScroll = useCallback(() => {
    if (rafRef.current !== null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      const element = scrollRef.current;
      if (!element) return;
      setCurrent(pageAt(offsetsRef.current, element.scrollTop));
    });
  }, []);

  useEffect(() => {
    offsetsRef.current = offsets;
  }, [offsets]);

  useEffect(() => {
    currentRef.current = current;
  }, [current]);

  // Восстановление позиции: ждём прогресс, чтобы не прыгнуть на первую страницу
  useEffect(() => {
    if (restoredRef.current || pages.length === 0 || width <= 0) return;
    if (progressQuery.isPending) return;

    // Прогресс хранится по каноническому ключу: при alias-URL сравниваем с ним
    const canonicalKey = contentQuery.data?.chapter_key ?? chapterKey;
    const saved = progressQuery.data?.progress ?? null;
    const savedPage = saved?.chapter_key === canonicalKey ? (saved.page ?? 0) : 0;
    const index = Math.min(Math.max(0, savedPage), pages.length - 1);

    const element = scrollRef.current;
    // Флаг ставим только в кадре: StrictMode отменяет первый rAF и повторяет
    // эффект — иначе восстановление не выполнилось бы в dev.
    const frame = requestAnimationFrame(() => {
      restoredRef.current = true;
      if (element) element.scrollTop = offsets[index] + 1;
      setCurrent(index);
    });
    return () => cancelAnimationFrame(frame);
  }, [
    pages.length,
    width,
    offsets,
    progressQuery.isPending,
    progressQuery.data,
    chapterKey,
    contentQuery.data?.chapter_key,
  ]);

  const mutateRef = useRef(save.mutate);
  useEffect(() => {
    mutateRef.current = save.mutate;
  }, [save.mutate]);

  // Обычное сохранение: дебаунс, чтобы скролл не писал в БД на каждый кадр
  useEffect(() => {
    if (!restoredRef.current || titleId === null || chapterKey === null) return;
    if (pages.length === 0) return;

    const timer = window.setTimeout(() => {
      mutateRef.current({ chapter_key: chapterKey, page: current });
    }, SAVE_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [current, titleId, chapterKey, pages.length]);

  // Последняя позиция при закрытии вкладки: keepalive, потому что unload
  useEffect(() => {
    if (titleId === null || chapterKey === null) return;

    const flush = () => {
      if (!restoredRef.current || currentRef.current < 0) return;
      void progressApi.flush(titleId, {
        chapter_key: chapterKey,
        page: currentRef.current,
      });
    };

    const onVisibility = () => {
      if (document.visibilityState === "hidden") flush();
    };

    window.addEventListener("pagehide", flush);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("pagehide", flush);
      document.removeEventListener("visibilitychange", onVisibility);
      flush();
    };
  }, [titleId, chapterKey]);

  // Клавиатура не должна перехватывать ввод в полях
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.isContentEditable ||
          ["INPUT", "TEXTAREA", "SELECT", "BUTTON", "A"].includes(target.tagName) ||
          target.closest(
            "a,button,[role='textbox'],[role='combobox'],[role='listbox'],[role='menu'],[role='dialog']",
          ) !== null)
      ) {
        return;
      }
      if (event.ctrlKey || event.altKey || event.metaKey) return;

      const element = scrollRef.current;
      const values = offsetsRef.current;
      if (!element || values.length < 2 || pages.length === 0) return;

      let next = currentRef.current;
      if (
        event.key === "ArrowRight" ||
        event.key === "ArrowDown" ||
        event.key === "PageDown" ||
        event.key === " "
      ) {
        next += 1;
      } else if (
        event.key === "ArrowLeft" ||
        event.key === "ArrowUp" ||
        event.key === "PageUp"
      ) {
        next -= 1;
      } else if (event.key === "Home") {
        next = 0;
      } else if (event.key === "End") {
        next = pages.length - 1;
      } else {
        return;
      }

      event.preventDefault();
      next = Math.min(Math.max(0, next), pages.length - 1);
      element.scrollTo({ top: values[next] + 1, behavior: "smooth" });
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [pages.length]);

  // Prefetch ближайших страниц: следующая должна быть готова до скролла
  useEffect(() => {
    for (let index = current + 1; index <= Math.min(pages.length - 1, current + 3); index += 1) {
      const image = new Image();
      image.src = pageImageUrl(pages[index].id);
    }
  }, [current, pages]);

  if (titleId === null) {
    return (
      <div className="grid min-h-dvh place-items-center p-6">
        <Link to="/" className={buttonVariants({ variant: "outline" })}>
          {t("reader.backToTitle")}
        </Link>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-40 flex flex-col bg-background text-foreground">
      <header className="flex shrink-0 items-center gap-2 border-b bg-background/95 px-2 py-2 backdrop-blur sm:px-4">
        <Link
          to={titlePath(titleId)}
          aria-label={t("reader.backToTitle")}
          className={buttonVariants({ variant: "ghost", size: "icon-sm" })}
        >
          <ArrowLeftIcon aria-hidden />
        </Link>

        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{chapterTitle}</p>
          <p className="truncate text-xs text-muted-foreground">
            {titleQuery.data?.title.name ?? ""}
            {contentQuery.data ? ` · ${contentQuery.data.source.name}` : ""}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          {previousChapter && (
            <Link
              to={readerPath(titleId, previousChapter.chapter_key)}
              aria-label={t("reader.prevChapter")}
              className={buttonVariants({ variant: "ghost", size: "icon-sm" })}
            >
              <ChevronLeftIcon aria-hidden />
            </Link>
          )}
          <span className="min-w-16 text-center text-xs text-muted-foreground tabular-nums">
            {pages.length > 0
              ? t("reader.pageOf", { page: current + 1, total: pages.length })
              : ""}
          </span>
          {nextChapter && (
            <Link
              to={readerPath(titleId, nextChapter.chapter_key)}
              aria-label={t("reader.nextChapter")}
              className={buttonVariants({ variant: "ghost", size: "icon-sm" })}
            >
              <ChevronRightIcon aria-hidden />
            </Link>
          )}
        </div>
      </header>

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="relative flex-1 overflow-y-auto overscroll-contain bg-muted/30"
      >
        {contentQuery.isPending && (
          <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 p-4">
            <Skeleton className="aspect-2/3 w-full rounded-lg" />
            <Skeleton className="aspect-2/3 w-full rounded-lg" />
          </div>
        )}

        {contentQuery.isError && (
          <div className="mx-auto flex max-w-md flex-col items-start gap-3 p-6">
            <p role="alert" className="text-sm text-destructive">
              {errorText}
            </p>
            <div className="flex gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => void contentQuery.refetch()}
              >
                {t("retry")}
              </Button>
              <Link
                to={titlePath(titleId)}
                className={buttonVariants({ variant: "ghost" })}
              >
                {t("reader.backToTitle")}
              </Link>
            </div>
          </div>
        )}

        {!contentQuery.isPending && !contentQuery.isError && content?.kind !== "images" && (
          <div className="mx-auto flex max-w-md flex-col items-start gap-3 p-6">
            <p className="text-sm text-muted-foreground">{t("reader.unsupported")}</p>
            <Link
              to={titlePath(titleId)}
              className={buttonVariants({ variant: "outline" })}
            >
              {t("reader.backToTitle")}
            </Link>
          </div>
        )}

        {content?.kind === "images" && pages.length > 0 && (
          <div className="mx-auto w-full max-w-3xl">
            <div style={{ height: offsets[start] }} />

            {pages.slice(start, end).map((page, offset) => {
              const index = start + offset;
              return (
                <div
                  key={page.id}
                  style={{ height: heights[index] }}
                  className="relative w-full"
                >
                  <img
                    src={pageImageUrl(page.id)}
                    alt={t("reader.pageAlt", { page: index + 1 })}
                    loading={index <= current + 2 ? "eager" : "lazy"}
                    decoding="async"
                    draggable={false}
                    className="h-full w-full object-contain"
                  />
                </div>
              );
            })}

            <div style={{ height: Math.max(0, totalHeight - offsets[end]) }} />

            <div className="flex items-center justify-between gap-2 p-4">
              {previousChapter ? (
                <Link
                  to={readerPath(titleId, previousChapter.chapter_key)}
                  className={buttonVariants({ variant: "outline" })}
                >
                  <ChevronLeftIcon aria-hidden />
                  {t("reader.prevChapter")}
                </Link>
              ) : (
                <span />
              )}
              {nextChapter ? (
                <Link
                  to={readerPath(titleId, nextChapter.chapter_key)}
                  className={buttonVariants({ variant: "outline" })}
                >
                  {t("reader.nextChapter")}
                  <ChevronRightIcon aria-hidden />
                </Link>
              ) : (
                <span />
              )}
            </div>

            <p className="pb-6 text-center text-xs text-muted-foreground">
              {t("reader.keyboardHint")}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
