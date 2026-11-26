import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useParams } from "react-router";
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";

import {
  pageImageUrl,
  progressApi,
  useChapterContentQuery,
  useErrorText,
  useLibraryTabsQuery,
  useProgressQuery,
  useSaveProgressMutation,
  useTitleQuery,
  useUpsertLibraryMutation,
  type ContentPage,
} from "@/api";
import type { LibraryStatus } from "@/lib/catalog";
import { isAvailableChapter, sortChapters } from "@/lib/chapters";
import { useT } from "@/lib/i18n";
import { READER_WIDTH_CLASS } from "@/lib/reader";
import { readerPath, titlePath } from "@/lib/routes";
import { useSetting } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { ReaderMobileControls, ReaderSideControls } from "./ReaderControls";
import { ReaderHeader } from "./ReaderHeader";

const EMPTY_PAGES: ContentPage[] = [];
const SAVE_DEBOUNCE_MS = 800;
const AROUND = 2;
const AHEAD = 6;
/** Небольшая прокрутка вниз прячет панели, как клик по странице. */
const CHROME_SCROLL_THRESHOLD = 32;
/** Программный скролл не должен считаться жестом читателя. */
const PROGRAMMATIC_SCROLL_MS = 500;

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
  const widthSetting = useSetting("readerWidth");

  const titleQuery = useTitleQuery(titleId);
  const contentQuery = useChapterContentQuery(titleId, chapterKey);
  const progressQuery = useProgressQuery(titleId);
  const save = useSaveProgressMutation(titleId ?? 0);
  const upsert = useUpsertLibraryMutation(titleId ?? 0);
  const tabsQuery = useLibraryTabsQuery();
  const errorText = useErrorText(contentQuery.error);
  const libraryError = useErrorText(upsert.error);

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

  const [columnWidth, setColumnWidth] = useState(0);
  const [current, setCurrent] = useState(0);
  const [chromeHidden, setChromeHidden] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const columnRef = useRef<HTMLDivElement>(null);
  const offsetsRef = useRef<number[]>([]);
  const currentRef = useRef(0);
  const restoredRef = useRef(false);
  const rafRef = useRef<number | null>(null);
  const lastScrollTopRef = useRef(0);
  const scrollAccumulatorRef = useRef(0);
  const suppressAutoHideUntilRef = useRef(0);

  const heights = useMemo(
    () =>
      pages.map((page) => {
        const ratio =
          page.width !== null && page.height !== null && page.width > 0
            ? page.height / page.width
            : 1.4;
        return Math.max(160, columnWidth * ratio);
      }),
    [pages, columnWidth],
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
  const currentChapter = chapterIndex >= 0 ? chapters[chapterIndex] : null;

  const numberText =
    currentChapter?.number_text ?? contentQuery.data?.number_text ?? null;
  const chapterLabel =
    numberText === null
      ? contentQuery.data
        ? t("title.special")
        : "…"
      : currentChapter?.volume != null
        ? `${currentChapter.volume}-${numberText}`
        : numberText;

  // Ширина колонки, а не скролла: от неё зависят высоты страниц
  useEffect(() => {
    const element = columnRef.current;
    if (!element) return;

    const update = () => setColumnWidth(element.clientWidth);
    update();

    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [pages.length]);

  // Программный скролл (восстановление позиции, ширина, клавиши) не должен
  // прятать панели: читатель их не двигал.
  const scrollToPosition = useCallback(
    (top: number, behavior: ScrollBehavior = "auto") => {
      const element = scrollRef.current;
      if (!element) return;
      suppressAutoHideUntilRef.current = performance.now() + PROGRAMMATIC_SCROLL_MS;
      lastScrollTopRef.current = top;
      scrollAccumulatorRef.current = 0;
      if (behavior === "smooth") {
        element.scrollTo({ top, behavior });
      } else {
        element.scrollTop = top;
      }
    },
    [],
  );

  const handleScroll = useCallback(() => {
    if (rafRef.current !== null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      const element = scrollRef.current;
      if (!element) return;

      const top = element.scrollTop;
      const delta = top - lastScrollTopRef.current;
      lastScrollTopRef.current = top;
      setCurrent(pageAt(offsetsRef.current, top));

      if (performance.now() < suppressAutoHideUntilRef.current) return;

      // Скролл вверх панели не показывает: состояние меняет только клик
      // по странице. Вниз — прячет после небольшого хода.
      if (delta <= 0) {
        scrollAccumulatorRef.current = 0;
        return;
      }
      scrollAccumulatorRef.current += delta;
      if (scrollAccumulatorRef.current > CHROME_SCROLL_THRESHOLD) {
        scrollAccumulatorRef.current = 0;
        setChromeHidden(true);
      }
    });
  }, []);

  useEffect(() => {
    offsetsRef.current = offsets;
  }, [offsets]);

  useEffect(() => {
    currentRef.current = current;
  }, [current]);

  // После смены ширины сохраняем текущую страницу, а не позицию в пикселях
  const previousColumnWidthRef = useRef(0);
  useEffect(() => {
    if (!restoredRef.current) {
      previousColumnWidthRef.current = columnWidth;
      return;
    }
    if (columnWidth <= 0 || previousColumnWidthRef.current === columnWidth) {
      return;
    }
    previousColumnWidthRef.current = columnWidth;
    scrollToPosition(offsetsRef.current[currentRef.current] + 1);
  }, [columnWidth, scrollToPosition]);

  // Восстановление позиции: ждём прогресс, чтобы не прыгнуть на первую страницу
  useEffect(() => {
    if (restoredRef.current || pages.length === 0 || columnWidth <= 0) return;
    if (progressQuery.isPending) return;

    // Прогресс хранится по каноническому ключу: при alias-URL сравниваем с ним
    const canonicalKey = contentQuery.data?.chapter_key ?? chapterKey;
    const saved = progressQuery.data?.progress ?? null;
    const savedPage = saved?.chapter_key === canonicalKey ? (saved.page ?? 0) : 0;
    const index = Math.min(Math.max(0, savedPage), pages.length - 1);

    // Флаг ставим только в кадре: StrictMode отменяет первый rAF и повторяет
    // эффект — иначе восстановление не выполнилось бы в dev.
    const frame = requestAnimationFrame(() => {
      restoredRef.current = true;
      scrollToPosition(offsets[index] + 1);
      setCurrent(index);
    });
    return () => cancelAnimationFrame(frame);
  }, [
    pages.length,
    columnWidth,
    offsets,
    progressQuery.isPending,
    progressQuery.data,
    chapterKey,
    contentQuery.data?.chapter_key,
    scrollToPosition,
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
      scrollToPosition(values[next] + 1, "smooth");
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [pages.length, scrollToPosition]);

  // Prefetch ближайших страниц: следующая должна быть готова до скролла
  useEffect(() => {
    for (let index = current + 1; index <= Math.min(pages.length - 1, current + 3); index += 1) {
      const image = new Image();
      image.src = pageImageUrl(pages[index].id);
    }
  }, [current, pages]);

  const library = titleQuery.data?.library ?? null;
  const tabs = tabsQuery.data?.tabs ?? [];
  const libraryPending = upsert.isPending;
  const libraryInput = {
    status: library?.status ?? "planned",
    score: library?.score ?? null,
    custom_tab_id: library?.custom_tab_id ?? null,
  } as const;

  function handleStatus(status: LibraryStatus): void {
    upsert.mutate({ ...libraryInput, status, custom_tab_id: null });
  }

  function handleCustomStatus(tabId: number): void {
    upsert.mutate({ ...libraryInput, custom_tab_id: tabId });
  }

  function addToLibrary(): void {
    upsert.mutate({ ...libraryInput, status: "planned", custom_tab_id: null });
  }

  if (titleId === null) {
    return (
      <div className="grid min-h-dvh place-items-center p-6">
        <Link to="/" className={buttonVariants({ variant: "outline" })}>
          {t("library.title")}
        </Link>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-40 overflow-hidden bg-background text-foreground">
      <ReaderHeader
        titleId={titleId}
        titleName={titleQuery.data?.title.name ?? ""}
        chapterLabel={chapterLabel}
        previousChapterKey={previousChapter?.chapter_key ?? null}
        nextChapterKey={nextChapter?.chapter_key ?? null}
        library={library}
        tabs={tabs}
        pending={libraryPending}
        errorText={libraryError}
        hidden={chromeHidden}
        onAdd={addToLibrary}
        onStatus={handleStatus}
        onCustomStatus={handleCustomStatus}
      />

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="h-dvh overflow-y-auto overscroll-contain bg-muted/30"
      >
        {contentQuery.isPending && (
          <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 px-4 pt-20 pb-10">
            <Skeleton className="aspect-2/3 w-full rounded-lg" />
            <Skeleton className="aspect-2/3 w-full rounded-lg" />
          </div>
        )}

        {contentQuery.isError && (
          <div className="mx-auto flex max-w-md flex-col items-start gap-3 p-6 pt-20">
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
          <div className="mx-auto flex max-w-md flex-col items-start gap-3 p-6 pt-20">
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
          <div
            ref={columnRef}
            className={cn("mx-auto w-full", READER_WIDTH_CLASS[widthSetting])}
          >
            <div style={{ height: offsets[start] }} />

            {pages.slice(start, end).map((page, offset) => {
              const index = start + offset;
              return (
                <div
                  key={page.id}
                  style={{ height: heights[index] }}
                  className="relative w-full"
                  onClick={() => setChromeHidden((value) => !value)}
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

            <div className="flex items-center justify-between gap-2 px-4 pt-4">
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

            <p className="px-4 pt-6 pb-32 text-center text-xs text-muted-foreground md:pb-10">
              {t("reader.keyboardHint")}
            </p>
          </div>
        )}
      </div>

      <ReaderSideControls page={current + 1} total={pages.length} />
      <ReaderMobileControls
        page={current + 1}
        total={pages.length}
        hidden={chromeHidden}
        library={library}
        tabs={tabs}
        pending={libraryPending}
        errorText={libraryError}
        onAdd={addToLibrary}
        onStatus={handleStatus}
        onCustomStatus={handleCustomStatus}
      />
    </div>
  );
}
