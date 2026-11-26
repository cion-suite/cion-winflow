import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  BookOpenIcon,
  CheckIcon,
  EyeIcon,
  EyeOffIcon,
  ImagesIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react";
import { Link, useParams } from "react-router";

import {
  formValuesToTitleInput,
  titleCoverUrl,
  titleToFormValues,
  useAuth,
  useErrorText,
  useLibraryTabsQuery,
  useProgressQuery,
  useRemoveLibraryMutation,
  useSetChapterReadMutation,
  useSyncTitleMutation,
  useTitleQuery,
  useUpdateTitleMutation,
  useUpsertLibraryMutation,
  type LibraryTab,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import {
  customStatusValue,
  customTabIdFromValue,
  LIBRARY_STATUSES,
  type LibraryStatus,
  type TitleFormValues,
} from "@/lib/catalog";
import { isAvailableChapter, sortChapters } from "@/lib/chapters";
import { useLocale, useT } from "@/lib/i18n";
import { readerPath } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { CoverImage } from "@/ui/CoverImage";
import { SourceIcon } from "@/ui/SourceIcon";
import { TitleForm } from "@/ui/TitleForm";
import { Badge } from "@/ui/shadcn/badge";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/ui/shadcn/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/ui/shadcn/dropdown-menu";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/ui/shadcn/empty";
import { Input } from "@/ui/shadcn/input";
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/ui/shadcn/item";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/ui/shadcn/tabs";
import { AliasesPanel } from "./AliasesPanel";
import { CoverEditorDialog } from "./CoverEditorDialog";
import { LibraryPanel } from "./LibraryPanel";

function chapterLabel(
  chapter: TitleChapter,
  chapterWord: string,
  specialWord: string,
): string {
  return chapter.number_text ? `${chapterWord} ${chapter.number_text}` : specialWord;
}

function makeDateFormatter(locale: string): (unix: number) => string {
  const formatter = new Intl.DateTimeFormat(locale === "ru" ? "ru-RU" : "en-GB", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  });
  return (unix) => formatter.format(new Date(unix * 1000));
}

function MetaRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[180px_1fr] sm:gap-3">
      <span className="text-sm text-muted-foreground">{label}</span>
      <div className="text-sm">{children}</div>
    </div>
  );
}

function ScoreSelect({
  value,
  disabled,
  onChange,
}: {
  value: number | null;
  disabled: boolean;
  onChange: (score: number | null) => void;
}) {
  const t = useT();
  const scoreOptions = [
    { value: null, label: t("library.noScore") },
    ...Array.from({ length: 10 }, (_, index) => ({
      value: index + 1,
      label: String(index + 1),
    })),
  ];

  return (
    <Select
      items={scoreOptions}
      value={value}
      disabled={disabled}
      onValueChange={(next) => onChange(next)}
    >
      <SelectTrigger size="sm" className="w-20" aria-label={t("library.score")}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {scoreOptions.map((option) => (
          <SelectItem key={option.value ?? "no-score"} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function RatingBlock({
  rating,
  score,
  disabled,
  onScore,
}: {
  rating: number | null;
  score: number | null;
  disabled: boolean;
  onScore: (score: number | null) => void;
}) {
  const t = useT();

  return (
    <div className="flex flex-col items-start gap-1 lg:items-end">
      {rating !== null ? (
        <span className="text-2xl font-semibold text-success">
          {rating.toFixed(1)}
        </span>
      ) : (
        <span className="text-sm text-muted-foreground">
          {t("title.noRating")}
        </span>
      )}
      <span className="text-xs text-muted-foreground">
        {t("title.sourceRating")}
      </span>
      <div className="flex items-center gap-2 pt-1">
        <span className="text-xs text-muted-foreground">
          {t("title.yourScore")}
        </span>
        <ScoreSelect value={score} disabled={disabled} onChange={onScore} />
      </div>
    </div>
  );
}

function TitleSources({
  sources,
  preferredSourceId,
}: {
  sources: TitleSource[];
  preferredSourceId: number | null;
}) {
  const t = useT();
  const visible = useMemo(
    () =>
      [...sources]
        .filter((source) => source.available_count > 0)
        .sort(
          (left, right) =>
            Number(right.source_id === preferredSourceId) -
              Number(left.source_id === preferredSourceId) ||
            Number(right.is_primary) - Number(left.is_primary) ||
            right.available_count - left.available_count ||
            left.name.localeCompare(right.name),
        ),
    [sources, preferredSourceId],
  );

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-medium">{t("title.sources")}</h2>
      {visible.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t("title.sourcesEmpty")}
        </p>
      ) : (
        <ItemGroup className="gap-2">
          {visible.map((source) => (
            <Item
              key={source.source_entry_id}
              variant="muted"
              size="sm"
              className="cursor-default"
            >
              <ItemMedia className="size-9 rounded-md bg-background p-1.5 ring-1 ring-border">
                <SourceIcon code={source.code} />
              </ItemMedia>
              <ItemContent>
                <ItemTitle>{source.name}</ItemTitle>
                <ItemDescription>
                  {t("title.sourceStats", {
                    chapters: source.chapter_count,
                    available: source.available_count,
                  })}
                </ItemDescription>
              </ItemContent>
              {!source.enabled && (
                <Badge variant="outline">{t("title.sourceDisabled")}</Badge>
              )}
            </Item>
          ))}
        </ItemGroup>
      )}
    </section>
  );
}

function ChapterRow({
  titleId,
  chapter,
  read,
  needsChoice,
  pending,
  formatDate,
  onMarkRead,
  onMarkUnread,
}: {
  titleId: number;
  chapter: TitleChapter;
  read: boolean;
  /** Есть непрочитанные предыдущие главы: глаз предлагает «эта и все предыдущие». */
  needsChoice: boolean;
  pending: boolean;
  formatDate: (unix: number) => string;
  onMarkRead: (withBelow: boolean) => void;
  onMarkUnread: () => void;
}) {
  const t = useT();
  // variants[0] может быть свежим locked preferred, а не выбранным вариантом.
  const variant =
    chapter.variants.find((item) => item.chapter_id === chapter.selected_chapter_id) ??
    chapter.variants[0];
  const source = variant?.source_name ?? "";
  const date = chapter.published_at === null ? null : formatDate(chapter.published_at);
  const eyeLabel = read ? t("title.markUnread") : t("title.markRead");

  return (
    <div
      className={cn(
        "flex items-center gap-1 rounded-xl border px-2 py-2.5 transition-colors hover:bg-muted/50",
        // Прочитанное не подсвечиваем фоном: приглушаем всю карточку, иначе
        // muted-фон в тёмной теме выглядит ярче непрочитанного.
        read && "text-muted-foreground opacity-50",
      )}
    >
      {needsChoice ? (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                className="shrink-0"
                aria-label={eyeLabel}
                disabled={pending}
              />
            }
          >
            <EyeIcon aria-hidden />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="min-w-max">
            <DropdownMenuGroup>
              <DropdownMenuItem onClick={() => onMarkRead(false)}>
                <EyeIcon aria-hidden />
                <span>{t("title.readOnlyChapter")}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onMarkRead(true)}>
                <EyeOffIcon aria-hidden />
                <span>{t("title.readChapterAndPrevious")}</span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      ) : (
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="shrink-0"
          aria-pressed={read}
          aria-label={eyeLabel}
          disabled={pending}
          onClick={read ? onMarkUnread : () => onMarkRead(false)}
        >
          {read ? <EyeOffIcon aria-hidden /> : <EyeIcon aria-hidden />}
        </Button>
      )}

      <Link
        to={readerPath(titleId, chapter.chapter_key)}
        className="flex min-w-0 flex-1 items-center gap-3 rounded-lg px-1 py-0.5 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        {chapter.volume !== null && (
          <span className="grid size-9 shrink-0 place-items-center rounded-lg border text-sm font-medium tabular-nums">
            {chapter.volume}
          </span>
        )}

        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">
            {chapterLabel(chapter, t("title.chapter"), t("title.special"))}
          </p>
          <p className="truncate text-xs text-muted-foreground">
            {[source, chapter.name].filter(Boolean).join(" · ")}
          </p>
        </div>

        {date && (
          <span className="hidden shrink-0 text-xs text-muted-foreground tabular-nums sm:inline">
            {date}
          </span>
        )}
      </Link>
    </div>
  );
}

export function TitlePage() {
  const t = useT();
  const locale = useLocale();
  const { id } = useParams();
  const { user } = useAuth();
  const parsed = Number(id);
  const titleId = Number.isInteger(parsed) && parsed > 0 ? parsed : null;

  const query = useTitleQuery(titleId);
  const progressQuery = useProgressQuery(titleId);
  const tabsQuery = useLibraryTabsQuery();
  const update = useUpdateTitleMutation(titleId ?? 0);
  const upsert = useUpsertLibraryMutation(titleId ?? 0);
  const remove = useRemoveLibraryMutation(titleId ?? 0);
  const setRead = useSetChapterReadMutation(titleId ?? 0);
  const sync = useSyncTitleMutation(titleId ?? 0);
  const syncedTitleIdRef = useRef<number | null>(null);

  // Страница показывает кэш сразу, а источники обновляются в фоне.
  // Сервер сам решает, устарели ли данные (не чаще раза в 10 минут).
  useEffect(() => {
    if (titleId === null || syncedTitleIdRef.current === titleId) return;
    syncedTitleIdRef.current = titleId;
    sync.mutate();
  }, [titleId, sync]);

  const [tab, setTab] = useState("chapters");
  const [editing, setEditing] = useState(false);
  const [coversOpen, setCoversOpen] = useState(false);
  const [chapterFilter, setChapterFilter] = useState("");
  const [ascending, setAscending] = useState(false);

  const queryError = useErrorText(query.error);
  const updateError = useErrorText(update.error);
  const readError = useErrorText(setRead.error);
  const mutationError = useErrorText(upsert.error ?? remove.error);
  const mutationPending = upsert.isPending || remove.isPending;
  const isAdmin = user?.role === "admin";

  const availableChapters = useMemo(
    () => (query.data?.chapters ?? []).filter(isAvailableChapter),
    [query.data?.chapters],
  );
  const readKeys = useMemo(
    () => new Set(query.data?.read_keys ?? []),
    [query.data?.read_keys],
  );

  // Порядок чтения: «эта глава и все предыдущие» — все главы до неё включительно
  const ascendingChapters = useMemo(
    () => sortChapters(availableChapters, true),
    [availableChapters],
  );
  const ascendingIndex = useMemo(
    () =>
      new Map(
        ascendingChapters.map((chapter, index) => [chapter.chapter_key, index]),
      ),
    [ascendingChapters],
  );
  // Главы, у которых есть непрочитанная предыдущая: для них глаз спрашивает
  const needsChoiceKeys = useMemo(() => {
    const keys = new Set<string>();
    let unreadEarlier = false;
    for (const chapter of ascendingChapters) {
      const unread = !readKeys.has(chapter.chapter_key);
      if (unread && unreadEarlier) keys.add(chapter.chapter_key);
      if (unread) unreadEarlier = true;
    }
    return keys;
  }, [ascendingChapters, readKeys]);

  const visibleChapters = useMemo(() => {
    const sorted = sortChapters(availableChapters, ascending);
    const needle = chapterFilter.trim().toLowerCase();
    if (needle === "") return sorted;
    return sorted.filter((chapter) =>
      [chapter.number_text, chapter.name, chapter.chapter_key]
        .filter((value): value is string => value !== null)
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
  }, [availableChapters, ascending, chapterFilter]);

  const firstChapterKey = ascendingChapters[0]?.chapter_key ?? null;
  const lastChapterKey =
    ascendingChapters[ascendingChapters.length - 1]?.chapter_key ?? null;

  const progressKey = progressQuery.data?.progress?.chapter_key ?? null;
  const readTarget = useMemo(() => {
    const saved = availableChapters.find(
      (chapter) => chapter.chapter_key === progressKey,
    );
    return saved?.chapter_key ?? firstChapterKey;
  }, [availableChapters, progressKey, firstChapterKey]);

  // «Прочитано» — только когда прогресс на последней главе и страница
  // последняя или предпоследняя. Одного факта открытия последней главы мало.
  const hasSavedProgress =
    progressKey !== null &&
    availableChapters.some((chapter) => chapter.chapter_key === progressKey);
  const lastChapterPageCount =
    ascendingChapters[ascendingChapters.length - 1]?.page_count ?? null;
  const progressPage = progressQuery.data?.progress?.page ?? null;
  const atLastPages =
    lastChapterPageCount !== null &&
    progressPage !== null &&
    progressPage >= Math.max(0, lastChapterPageCount - 2);
  const finished =
    hasSavedProgress && readTarget === lastChapterKey && atLastPages;
  const readLabel = finished
    ? t("title.finished")
    : hasSavedProgress && readTarget !== firstChapterKey
      ? t("title.continue")
      : t("title.read");

  const formatDate = useMemo(() => makeDateFormatter(locale), [locale]);
  const tabs: LibraryTab[] = tabsQuery.data?.tabs ?? [];

  if (titleId === null) {
    return (
      <Card>
        <CardContent>
          <p role="alert" className="text-sm text-destructive">
            {t("errors.titleNotFound")}
          </p>
        </CardContent>
      </Card>
    );
  }

  if (query.isPending) {
    return (
      <div className="mx-auto grid w-full max-w-6xl gap-8 md:grid-cols-[260px_1fr]">
        <Skeleton className="aspect-2/3 w-full rounded-xl" />
        <div className="flex flex-col gap-3">
          <Skeleton className="h-4 w-1/4" />
          <Skeleton className="h-8 w-1/2" />
          <Skeleton className="h-10 w-full rounded-xl" />
          <Skeleton className="h-64 w-full rounded-xl" />
        </div>
      </div>
    );
  }

  if (query.isError || !query.data) {
    return (
      <Card>
        <CardContent className="flex flex-col items-start gap-2">
          <p role="alert" className="text-sm text-destructive">
            {queryError}
          </p>
          <Button type="button" variant="outline" onClick={() => void query.refetch()}>
            {t("retry")}
          </Button>
        </CardContent>
      </Card>
    );
  }

  const { title, library, sources, aliases } = query.data;
  const statusItems = [
    ...LIBRARY_STATUSES.map((status) => ({
      value: status as string,
      label: t(`library.statuses.${status}`),
    })),
    ...tabs.map((tab) => ({
      value: customStatusValue(tab.id),
      label: tab.name,
    })),
  ];
  // Если закладка уже удалена, а кэш ещё старый, показываем встроенный
  // статус, а не пустой триггер с несуществующим custom:<id>.
  const selectedCustomTabId = library?.custom_tab_id ?? null;
  const selectedCustomTab =
    selectedCustomTabId === null
      ? undefined
      : tabs.find((tab) => tab.id === selectedCustomTabId);
  const selectedStatus = selectedCustomTab
    ? customStatusValue(selectedCustomTab.id)
    : (library?.status ?? "planned");
  const meta = [
    t(`catalog.types.${title.kind}`),
    title.year === null ? null : String(title.year),
    title.status === null ? null : t(`title.statuses.${title.status}`),
  ]
    .filter((part) => part !== null)
    .join(" · ");

  function currentInput() {
    return {
      status: library?.status ?? "planned",
      score: library?.score ?? null,
      custom_tab_id: library?.custom_tab_id ?? null,
    };
  }

  function handleStatusValue(value: string | null): void {
    if (value === null) return;
    const customTabId = customTabIdFromValue(value);
    upsert.mutate(
      customTabId === null
        ? { ...currentInput(), status: value as LibraryStatus, custom_tab_id: null }
        : { ...currentInput(), custom_tab_id: customTabId },
    );
  }

  function handleScore(score: number | null): void {
    upsert.mutate({ ...currentInput(), score });
  }

  function addToLibrary(): void {
    upsert.mutate({ ...currentInput(), custom_tab_id: null });
  }

  function handleMarkRead(chapter: TitleChapter, withBelow: boolean): void {
    const index = ascendingIndex.get(chapter.chapter_key);
    const keys =
      withBelow && index !== undefined
        ? ascendingChapters.slice(0, index + 1).map((item) => item.chapter_key)
        : [chapter.chapter_key];
    setRead.mutate({ chapter_keys: keys, read: true });
  }

  function handleMarkUnread(chapter: TitleChapter): void {
    setRead.mutate({ chapter_keys: [chapter.chapter_key], read: false });
  }

  function toggleEditing(): void {
    update.reset();
    setEditing((value) => !value);
    setTab("main");
  }

  function handleUpdate(values: TitleFormValues): void {
    update.mutate(formValuesToTitleInput(values), {
      onSuccess: () => setEditing(false),
    });
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-8">
      <div className="grid gap-8 md:grid-cols-[260px_minmax(0,1fr)] lg:grid-cols-[260px_minmax(0,1fr)_240px]">
        <aside className="flex flex-col gap-3 md:sticky md:top-6 md:self-start">
          <CoverImage
            src={title.has_cover ? titleCoverUrl(title, "high") : null}
            alt={title.name}
            loading="eager"
            className="aspect-2/3 w-full rounded-xl ring-1 ring-foreground/10"
          />

          {readTarget && !finished ? (
            <Link
              to={readerPath(titleId, readTarget)}
              className={cn(buttonVariants(), "w-full")}
            >
              <BookOpenIcon data-icon="inline-start" />
              {readLabel}
            </Link>
          ) : (
            <Button type="button" className="w-full" disabled>
              {finished ? (
                <CheckIcon data-icon="inline-start" />
              ) : (
                <BookOpenIcon data-icon="inline-start" />
              )}
              {readLabel}
            </Button>
          )}

          {library ? (
            <Select
              items={statusItems}
              value={selectedStatus}
              disabled={mutationPending}
              onValueChange={handleStatusValue}
            >
              <SelectTrigger className="w-full" aria-label={t("library.status")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {LIBRARY_STATUSES.map((status) => (
                    <SelectItem key={status} value={status}>
                      {t(`library.statuses.${status}`)}
                    </SelectItem>
                  ))}
                </SelectGroup>
                {tabs.length > 0 && (
                  <>
                    <SelectSeparator />
                    <SelectGroup>
                      <SelectLabel>{t("library.customTabs")}</SelectLabel>
                      {tabs.map((tab) => (
                        <SelectItem
                          key={customStatusValue(tab.id)}
                          value={customStatusValue(tab.id)}
                        >
                          {tab.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </>
                )}
              </SelectContent>
            </Select>
          ) : (
            <div className="flex flex-col gap-1.5">
              <Button
                type="button"
                variant="secondary"
                className="w-full"
                disabled={mutationPending}
                onClick={addToLibrary}
              >
                <PlusIcon data-icon="inline-start" />
                {t("title.addToLibrary")}
              </Button>
              {mutationError && (
                <p role="alert" className="text-sm text-destructive">
                  {mutationError}
                </p>
              )}
            </div>
          )}

          {isAdmin && (
            <Button
              type="button"
              variant="ghost"
              className="w-full"
              onClick={toggleEditing}
            >
              {t("title.edit")}
            </Button>
          )}

          {library && (
            <LibraryPanel
              pending={mutationPending}
              errorText={mutationError}
              onRemove={() => remove.mutate()}
            />
          )}

          {isAdmin && (
            <Button
              type="button"
              variant="ghost"
              className="w-full"
              onClick={() => setCoversOpen(true)}
            >
              <ImagesIcon data-icon="inline-start" />
              {t("title.covers")}
            </Button>
          )}
        </aside>

        <div className="flex min-w-0 flex-col gap-4">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-1">
              <p className="text-sm text-muted-foreground">{meta}</p>
              <h1 className="text-2xl font-semibold tracking-tight">{title.name}</h1>
              <p className="text-sm text-muted-foreground">
                {t("title.chapterCount", { count: availableChapters.length })}
              </p>
            </div>

            <div className="shrink-0 lg:hidden">
              <RatingBlock
                rating={title.rating}
                score={library?.score ?? null}
                disabled={mutationPending}
                onScore={handleScore}
              />
            </div>
          </div>

          <Tabs value={tab} onValueChange={setTab}>
            <TabsList
              variant="line"
              className="w-full justify-start overflow-x-auto scrollbar-none"
            >
              <TabsTrigger value="chapters" className="flex-none">
                {t("title.chapters")} ({availableChapters.length})
              </TabsTrigger>
              <TabsTrigger value="main" className="flex-none">
                {t("title.main")}
              </TabsTrigger>
            </TabsList>

            <TabsContent value="chapters">
              <div className="flex flex-col gap-3">
                <div className="flex flex-col gap-2 sm:flex-row">
                  <div className="relative flex-1">
                    <SearchIcon
                      aria-hidden
                      className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
                    />
                    <Input
                      value={chapterFilter}
                      placeholder={t("title.chapterSearch")}
                      aria-label={t("title.chapterSearch")}
                      className="pl-8"
                      onChange={(event) => setChapterFilter(event.target.value)}
                    />
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => setAscending((value) => !value)}
                  >
                    {ascending ? (
                      <ArrowUpIcon data-icon="inline-start" />
                    ) : (
                      <ArrowDownIcon data-icon="inline-start" />
                    )}
                    {ascending ? t("title.showFromStart") : t("title.showNewest")}
                  </Button>
                </div>

                {readError && (
                  <p role="alert" className="text-sm text-destructive">
                    {readError}
                  </p>
                )}

                {visibleChapters.length === 0 ? (
                  <Empty className="border">
                    <EmptyHeader>
                      <EmptyMedia variant="icon">
                        <BookOpenIcon />
                      </EmptyMedia>
                      <EmptyTitle>{t("title.chaptersEmpty")}</EmptyTitle>
                      <EmptyDescription>
                        {chapterFilter === ""
                          ? t("title.chaptersEmptyDescription")
                          : t("title.chaptersNoMatch")}
                      </EmptyDescription>
                    </EmptyHeader>
                  </Empty>
                ) : (
                  <div className="flex flex-col gap-1.5">
                    {visibleChapters.map((chapter) => (
                      <ChapterRow
                        key={chapter.chapter_key}
                        titleId={titleId}
                        chapter={chapter}
                        read={readKeys.has(chapter.chapter_key)}
                        needsChoice={
                          !readKeys.has(chapter.chapter_key) &&
                          needsChoiceKeys.has(chapter.chapter_key)
                        }
                        pending={setRead.isPending}
                        formatDate={formatDate}
                        onMarkRead={(withBelow) =>
                          handleMarkRead(chapter, withBelow)
                        }
                        onMarkUnread={() => handleMarkUnread(chapter)}
                      />
                    ))}
                  </div>
                )}
              </div>
            </TabsContent>

            <TabsContent value="main">
              <div className="flex flex-col gap-4">
                {editing && isAdmin && (
                  <Card>
                    <CardHeader>
                      <CardTitle>{t("title.editTitle")}</CardTitle>
                    </CardHeader>
                    <CardContent>
                      <TitleForm
                        initial={titleToFormValues(title)}
                        submitLabel={t("title.save")}
                        isPending={update.isPending}
                        errorText={updateError}
                        onSubmit={handleUpdate}
                        onCancel={() => {
                          update.reset();
                          setEditing(false);
                        }}
                      />
                    </CardContent>
                  </Card>
                )}

                <div className="rounded-xl border p-4 text-sm whitespace-pre-line">
                  {title.description === null ? (
                    <span className="text-muted-foreground">
                      {t("title.noDescription")}
                    </span>
                  ) : (
                    title.description
                  )}
                </div>

                <div className="flex flex-col gap-3">
                  {title.alt_names.length > 0 && (
                    <MetaRow label={t("title.altNames")}>
                      {title.alt_names.join(", ")}
                    </MetaRow>
                  )}
                  {title.authors.length > 0 && (
                    <MetaRow label={t("title.authors")}>
                      {title.authors.join(", ")}
                    </MetaRow>
                  )}
                  {title.genres.length > 0 && (
                    <MetaRow label={t("title.tags")}>
                      <div className="flex flex-wrap gap-1.5">
                        {title.genres.map((tag) => (
                          <Badge key={tag} variant="secondary">
                            {tag}
                          </Badge>
                        ))}
                      </div>
                    </MetaRow>
                  )}
                  {title.lang !== null && (
                    <MetaRow label={t("title.lang")}>{title.lang}</MetaRow>
                  )}
                  {title.status !== null && (
                    <MetaRow label={t("title.status")}>
                      {t(`title.statuses.${title.status}`)}
                    </MetaRow>
                  )}
                  {title.year !== null && (
                    <MetaRow label={t("title.year")}>{title.year}</MetaRow>
                  )}
                </div>

                {isAdmin && (
                  <AliasesPanel titleId={titleId} chapters={query.data.chapters} aliases={aliases} />
                )}
              </div>
            </TabsContent>
          </Tabs>
        </div>

        <aside className="md:col-span-2 lg:col-span-1 lg:sticky lg:top-6 lg:self-start">
          <div className="flex flex-col gap-6">
            <div className="hidden lg:flex lg:justify-end">
              <RatingBlock
                rating={title.rating}
                score={library?.score ?? null}
                disabled={mutationPending}
                onScore={handleScore}
              />
            </div>
            <TitleSources
              sources={sources}
              preferredSourceId={title.preferred_source_id}
            />
          </div>
        </aside>
      </div>

      {isAdmin && (
        <CoverEditorDialog
          title={title}
          sources={sources}
          open={coversOpen}
          onOpenChange={setCoversOpen}
        />
      )}
    </div>
  );
}
