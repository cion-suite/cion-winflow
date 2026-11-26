import { useState } from "react";
import { LibraryBigIcon, PencilIcon, PlusIcon } from "lucide-react";

import {
  useErrorText,
  useLibraryQuery,
  useLibraryTabsQuery,
  type LibraryQuery,
  type LibrarySort,
  type LibraryTab,
} from "@/api";
import type { LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Pagination } from "@/ui/Pagination";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent } from "@/ui/shadcn/card";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/ui/shadcn/empty";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/ui/shadcn/tabs";
import { AddTitleDialog } from "./AddTitleDialog";
import { LibraryCard } from "./LibraryCard";
import { CreateTabDialog, ManageTabsDialog } from "./TabDialogs";

const PAGE_SIZE = 24;
const DEFAULT_TAB_LIMIT = 3;

/** Порядок вкладок как в макете: «Читаю» левее «Буду читать». */
const STATUS_ORDER: LibraryStatus[] = [
  "in_progress",
  "planned",
  "completed",
  "dropped",
  "on_hold",
  "not_interested",
];

/** Удалённая закладка не должна оставлять пустую вкладку выбранной. */
function resolveView(view: string, tabs: LibraryTab[] | undefined): string {
  if (!view.startsWith("tab:") || tabs === undefined) return view;
  const tabId = Number(view.slice("tab:".length));
  return tabs.some((tab) => tab.id === tabId) ? view : "all";
}

function viewToQuery(
  view: string,
): Pick<LibraryQuery, "status" | "favorite" | "tabId"> {
  if (view === "favorite") return { favorite: true };
  if (view.startsWith("status:")) {
    return { status: view.slice("status:".length) as LibraryStatus };
  }
  if (view.startsWith("tab:")) {
    return { tabId: Number(view.slice("tab:".length)) };
  }
  return {};
}

export function LibraryPage() {
  const t = useT();
  const [view, setView] = useState("all");
  const [sort, setSort] = useState<LibrarySort>("updated");
  const [page, setPage] = useState(1);
  const [addOpen, setAddOpen] = useState(false);
  const [createTabOpen, setCreateTabOpen] = useState(false);
  const [manageTabsOpen, setManageTabsOpen] = useState(false);

  const tabsQuery = useLibraryTabsQuery();
  const customTabs = tabsQuery.data?.tabs ?? [];
  const effectiveView = resolveView(view, tabsQuery.data?.tabs);
  const library = useLibraryQuery({
    ...viewToQuery(effectiveView),
    sort,
    page,
    pageSize: PAGE_SIZE,
  });
  const errorText = useErrorText(library.error);

  const items = library.data?.items ?? [];
  const total = library.data?.total ?? 0;
  const pageSize = library.data?.page_size ?? PAGE_SIZE;
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const counts = library.data?.counts;
  const tabLimit = tabsQuery.data?.limit ?? DEFAULT_TAB_LIMIT;
  const isFiltered = effectiveView !== "all";

  const tabDefs = [
    { value: "all", label: t("library.tabs.all"), count: counts?.all ?? 0 },
    ...STATUS_ORDER.map((status) => ({
      value: `status:${status}`,
      label: t(`library.statuses.${status}`),
      count: counts?.statuses[status] ?? 0,
    })),
    {
      value: "favorite",
      label: t("library.favorite"),
      count: counts?.favorite ?? 0,
    },
    ...customTabs.map((tab) => ({
      value: `tab:${tab.id}`,
      label: tab.name,
      count: tab.count,
    })),
  ];

  function changeView(next: string): void {
    setView(next);
    setPage(1);
  }

  function changeSort(next: LibrarySort): void {
    setSort(next);
    setPage(1);
  }

  const sortOptions = [
    { value: "updated", label: t("library.sorts.updated") },
    { value: "added", label: t("library.sorts.added") },
    { value: "title", label: t("library.sorts.title") },
    { value: "score", label: t("library.sorts.score") },
  ] as const;

  return (
    <div className="flex w-full flex-col gap-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-2">
          <h1 className="text-2xl font-semibold tracking-tight">
            {t("library.title")}
          </h1>
          <span className="h-1 w-16 rounded-full bg-primary" aria-hidden />
        </div>
        <Button type="button" onClick={() => setAddOpen(true)}>
          <PlusIcon data-icon="inline-start" />
          {t("library.addTitle")}
        </Button>
      </div>

      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1 overflow-x-auto scrollbar-none">
          <Tabs value={effectiveView} onValueChange={changeView}>
            <TabsList variant="line" className="w-max justify-start">
              {tabDefs.map((tab) => (
                <TabsTrigger
                  key={tab.value}
                  value={tab.value}
                  className="flex-none gap-1.5 px-2"
                >
                  {tab.label}
                  <span
                    className={cn(
                      "rounded-full px-1.5 py-0.5 text-xs tabular-nums",
                      effectiveView === tab.value
                        ? "bg-primary text-primary-foreground"
                        : "bg-muted text-muted-foreground",
                    )}
                  >
                    {tab.count}
                  </span>
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            aria-label={t("library.tabs.createTitle")}
            title={
              customTabs.length >= tabLimit
                ? t("library.tabs.limit", { count: tabLimit })
                : t("library.tabs.createTitle")
            }
            disabled={customTabs.length >= tabLimit}
            onClick={() => setCreateTabOpen(true)}
          >
            <PlusIcon />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            aria-label={t("library.tabs.manageTitle")}
            title={t("library.tabs.manageTitle")}
            onClick={() => setManageTabsOpen(true)}
          >
            <PencilIcon />
          </Button>
        </div>
      </div>

      <div className="flex justify-end">
        <Select
          items={sortOptions}
          value={sort}
          onValueChange={(value) => changeSort(value as LibrarySort)}
        >
          <SelectTrigger className="w-48" aria-label={t("library.sortLabel")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {sortOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {library.isPending && (
        <div className="grid grid-cols-2 gap-x-4 gap-y-6 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
          {Array.from({ length: 12 }, (_, index) => (
            <div key={index} className="flex flex-col gap-2">
              <Skeleton className="aspect-2/3 w-full rounded-xl" />
              <Skeleton className="h-3 w-2/3" />
              <Skeleton className="h-4 w-5/6" />
            </div>
          ))}
        </div>
      )}

      {library.isError && (
        <Card>
          <CardContent className="flex flex-col items-start gap-2">
            <p role="alert" className="text-sm text-destructive">
              {errorText}
            </p>
            <Button type="button" variant="outline" onClick={() => void library.refetch()}>
              {t("retry")}
            </Button>
          </CardContent>
        </Card>
      )}

      {!library.isPending && !library.isError && items.length === 0 && (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <LibraryBigIcon />
            </EmptyMedia>
            <EmptyTitle>
              {isFiltered ? t("library.emptyTab") : t("library.empty")}
            </EmptyTitle>
            <EmptyDescription>
              {isFiltered
                ? t("library.emptyTabDescription")
                : t("library.emptyDescription")}
            </EmptyDescription>
          </EmptyHeader>
          {!isFiltered && (
            <EmptyContent>
              <Button type="button" onClick={() => setAddOpen(true)}>
                <PlusIcon data-icon="inline-start" />
                {t("library.addTitle")}
              </Button>
            </EmptyContent>
          )}
        </Empty>
      )}

      {!library.isError && items.length > 0 && (
        <div className="grid grid-cols-2 gap-x-4 gap-y-6 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
          {items.map((item) => (
            <LibraryCard key={item.title_id} item={item} />
          ))}
        </div>
      )}

      <Pagination page={page} pages={pages} onPageChange={setPage} />

      <AddTitleDialog open={addOpen} onOpenChange={setAddOpen} />
      <CreateTabDialog open={createTabOpen} onOpenChange={setCreateTabOpen} />
      <ManageTabsDialog open={manageTabsOpen} onOpenChange={setManageTabsOpen} />
    </div>
  );
}
