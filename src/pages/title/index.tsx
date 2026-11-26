import { type ReactNode, useMemo, useState } from "react";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  BookOpenIcon,
  EyeIcon,
  HeartIcon,
  ImagesIcon,
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
  useSetTitleTabsMutation,
  useTitleQuery,
  useUpdateTitleMutation,
  useUpsertLibraryMutation,
  type LibraryTab,
  type TitleChapter,
} from "@/api";
import type { LibraryStatus, TitleFormValues } from "@/lib/catalog";
import { isAvailableChapter, sortChapters } from "@/lib/chapters";
import { useLocale, useT } from "@/lib/i18n";
import { readerPath } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { CoverImage } from "@/ui/CoverImage";
import { TitleForm } from "@/ui/TitleForm";
import { Badge } from "@/ui/shadcn/badge";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/ui/shadcn/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/ui/shadcn/empty";
import { Input } from "@/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/ui/shadcn/tabs";
import { AdminSourcePanel } from "./AdminSourcePanel";
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

function ChapterRow({
  titleId,
  chapter,
  read,
  formatDate,
}: {
  titleId: number;
  chapter: TitleChapter;
  read: boolean;
  formatDate: (unix: number) => string;
}) {
  const t = useT();
  // variants[0] может быть свежим locked preferred, а не выбранным вариантом.
  const variant =
    chapter.variants.find((item) => item.chapter_id === chapter.selected_chapter_id) ??
    chapter.variants[0];
  const source = variant?.source_name ?? "";
  const date = chapter.published_at === null ? null : formatDate(chapter.published_at);

  return (
    <Link
      to={readerPath(titleId, chapter.chapter_key)}
      className={cn(
        "flex items-center gap-3 rounded-xl border px-3 py-2.5 transition-colors outline-none",
        "hover:bg-muted/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50",
      )}
    >
      <span
        className="grid size-8 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground"
        title={read ? t("title.chapterRead") : t("title.chapterUnread")}
      >
        <EyeIcon className={cn("size-4", read && "text-primary")} aria-hidden />
        <span className="sr-only">
          {read ? t("title.chapterRead") : t("title.chapterUnread")}
        </span>
      </span>

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
  const setTabs = useSetTitleTabsMutation(titleId ?? 0);

  const [tab, setTab] = useState("chapters");
  const [editing, setEditing] = useState(false);
  const [coversOpen, setCoversOpen] = useState(false);
  const [chapterFilter, setChapterFilter] = useState("");
  const [ascending, setAscending] = useState(false);

  const queryError = useErrorText(query.error);
  const updateError = useErrorText(update.error);
  const mutationError = useErrorText(upsert.error ?? remove.error ?? setTabs.error);
  const mutationPending = upsert.isPending || remove.isPending || setTabs.isPending;
  const isAdmin = user?.role === "admin";

  const availableChapters = useMemo(
    () => (query.data?.chapters ?? []).filter(isAvailableChapter),
    [query.data?.chapters],
  );
  const readKeys = useMemo(
    () => new Set(query.data?.read_keys ?? []),
    [query.data?.read_keys],
  );

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

  const firstChapterKey = useMemo(() => {
    const ascendingChapters = sortChapters(availableChapters, true);
    return ascendingChapters[0]?.chapter_key ?? null;
  }, [availableChapters]);

  const progressKey = progressQuery.data?.progress?.chapter_key ?? null;
  const readTarget = useMemo(() => {
    const saved = availableChapters.find(
      (chapter) => chapter.chapter_key === progressKey,
    );
    return saved?.chapter_key ?? firstChapterKey;
  }, [availableChapters, progressKey, firstChapterKey]);

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
  const favorite = library?.favorite ?? false;
  const meta = [
    t(`catalog.kinds.${title.kind}`),
    title.year === null ? null : String(title.year),
    title.status === null ? null : t(`title.statuses.${title.status}`),
  ]
    .filter((part) => part !== null)
    .join(" · ");

  function currentInput() {
    return {
      status: library?.status ?? "planned",
      score: library?.score ?? null,
      favorite,
    };
  }

  function handleStatus(status: LibraryStatus): void {
    upsert.mutate({ ...currentInput(), status });
  }

  function handleScore(score: number | null): void {
    upsert.mutate({ ...currentInput(), score });
  }

  function toggleFavorite(): void {
    upsert.mutate({ ...currentInput(), favorite: !favorite });
  }

  function addToLibrary(): void {
    upsert.mutate(currentInput());
  }

  function toggleTab(tabId: number): void {
    const current = library?.tab_ids ?? [];
    setTabs.mutate(
      current.includes(tabId)
        ? current.filter((value) => value !== tabId)
        : [...current, tabId],
    );
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
      <div className="grid gap-8 md:grid-cols-[260px_minmax(0,1fr)]">
        <aside className="flex flex-col gap-3 md:sticky md:top-6 md:self-start">
          <CoverImage
            src={title.has_cover ? titleCoverUrl(title, "high") : null}
            alt={title.name}
            loading="eager"
            className="aspect-2/3 w-full rounded-xl ring-1 ring-foreground/10"
          />

          {readTarget ? (
            <Link
              to={readerPath(titleId, readTarget)}
              className={cn(buttonVariants(), "w-full")}
            >
              <BookOpenIcon data-icon="inline-start" />
              {t("title.read")}
            </Link>
          ) : (
            <Button type="button" className="w-full" disabled>
              <BookOpenIcon data-icon="inline-start" />
              {t("title.read")}
            </Button>
          )}

          <Button
            type="button"
            variant={favorite ? "default" : "secondary"}
            className="w-full"
            aria-pressed={favorite}
            disabled={mutationPending}
            onClick={toggleFavorite}
          >
            <HeartIcon
              data-icon="inline-start"
              className={favorite ? "fill-current" : undefined}
            />
            {t("library.favorite")}
          </Button>

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

          <LibraryPanel
            entry={library}
            tabs={tabs}
            pending={mutationPending}
            errorText={mutationError}
            onAdd={addToLibrary}
            onStatus={handleStatus}
            onToggleTab={toggleTab}
            onRemove={() => remove.mutate()}
          />

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

            <div className="flex shrink-0 flex-col items-end gap-1">
              {title.rating !== null ? (
                <span className="text-2xl font-semibold text-success">
                  {title.rating.toFixed(1)}
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
                <ScoreSelect
                  value={library?.score ?? null}
                  disabled={mutationPending}
                  onChange={handleScore}
                />
              </div>
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
                        formatDate={formatDate}
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
                    <MetaRow label={t("title.genres")}>
                      {title.genres.join(", ")}
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

                {isAdmin && sources.length > 0 && (
                  <Card>
                    <CardHeader>
                      <CardTitle>{t("title.sources")}</CardTitle>
                    </CardHeader>
                    <CardContent className="flex flex-col gap-2">
                      {sources.map((source) => (
                        <div
                          key={source.source_entry_id}
                          className="flex flex-col gap-1"
                        >
                          <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
                            <div className="flex items-center gap-2">
                              <span className="font-medium">{source.name}</span>
                              {source.is_primary && (
                                <Badge variant="secondary">
                                  {t("title.primarySource")}
                                </Badge>
                              )}
                              {title.preferred_source_id === source.source_id && (
                                <Badge>{t("title.preferredSource")}</Badge>
                              )}
                              {!source.enabled && (
                                <Badge variant="outline">
                                  {t("title.sourceDisabled")}
                                </Badge>
                              )}
                            </div>
                            <span className="text-muted-foreground">
                              {t("title.sourceStats", {
                                chapters: source.chapter_count,
                                available: source.available_count,
                              })}
                            </span>
                          </div>
                          {source.last_error !== null && (
                            <p className="text-xs text-destructive">
                              {t("title.sourceError", {
                                message: source.last_error,
                              })}
                            </p>
                          )}
                        </div>
                      ))}
                    </CardContent>
                  </Card>
                )}

                {isAdmin && (
                  <AdminSourcePanel
                    titleId={titleId}
                    titleKind={title.kind}
                    sources={sources}
                    preferredSourceId={title.preferred_source_id}
                  />
                )}

                {isAdmin && (
                  <AliasesPanel titleId={titleId} chapters={query.data.chapters} aliases={aliases} />
                )}
              </div>
            </TabsContent>
          </Tabs>
        </div>
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
