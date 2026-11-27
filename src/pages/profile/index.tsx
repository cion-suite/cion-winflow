import { useState } from "react";
import { LibraryBigIcon, UserRoundIcon } from "lucide-react";
import { useParams } from "react-router";

import {
  titleCoverUrl,
  useAuth,
  useErrorText,
  useProfileQuery,
} from "@/api";
import { LIBRARY_STATUSES, type LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { LibraryCard } from "@/ui/LibraryCard";
import { Pagination } from "@/ui/Pagination";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent } from "@/ui/shadcn/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/ui/shadcn/empty";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/ui/shadcn/tabs";
import { ProfileHeader } from "./ProfileHeader";

const PAGE_SIZE = 12;

/** «Не интересно» — отказ, а не коллекция: в публичном профиле его нет. */
const PROFILE_STATUSES = LIBRARY_STATUSES.filter(
  (status) => status !== "not_interested",
);

function statusFromView(view: string): LibraryStatus | undefined {
  return view.startsWith("status:")
    ? (view.slice("status:".length) as LibraryStatus)
    : undefined;
}

export function ProfilePage() {
  const t = useT();
  const { id } = useParams();
  const { user: current } = useAuth();

  const routeId = id === undefined ? current?.id : Number(id);
  const userId =
    typeof routeId === "number" &&
    Number.isInteger(routeId) &&
    routeId > 0
      ? routeId
      : null;
  const isOwn = userId !== null && current?.id === userId;

  // Фильтр и страница привязаны к uid: при переходе на другой профиль
  // выбор не переносится, а сбрасывается без эффекта.
  const [selection, setSelection] = useState({
    userId,
    view: "all",
    page: 1,
  });
  const active =
    selection.userId === userId
      ? selection
      : { userId, view: "all", page: 1 };
  const { view, page } = active;

  const profile = useProfileQuery(userId, {
    status: statusFromView(view),
    page,
    pageSize: PAGE_SIZE,
  });
  const errorText = useErrorText(profile.error);

  if (userId === null) {
    return (
      <NotFound
        title={t("profile.userNotFound")}
        description={t("profile.userNotFoundDescription")}
      />
    );
  }

  if (profile.isPending) {
    return <ProfileSkeleton />;
  }

  if (profile.isError || !profile.data) {
    return (
      <Card>
        <CardContent className="flex flex-col items-start gap-2">
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
          <Button
            type="button"
            variant="outline"
            onClick={() => void profile.refetch()}
          >
            {t("retry")}
          </Button>
        </CardContent>
      </Card>
    );
  }

  const { user, stats, library } = profile.data;
  const items = library.items;
  const pages = Math.max(1, Math.ceil(library.total / library.page_size));
  const isFiltered = view !== "all";

  const tabDefs = [
    { value: "all", label: t("library.tabs.all"), count: stats.counts.all },
    ...PROFILE_STATUSES.map((status) => ({
      value: `status:${status}`,
      label: t(`library.statuses.${status}`),
      count: stats.counts.statuses[status],
    })),
  ];

  function changeView(next: string): void {
    setSelection({ userId, view: next, page: 1 });
  }

  function changePage(next: number): void {
    setSelection({ userId, view, page: next });
  }

  return (
    <div className="flex w-full flex-col gap-5">
      <ProfileHeader
        user={user}
        readChapters={stats.read_chapters}
        titles={stats.counts.all}
        completed={stats.counts.statuses.completed}
        isOwn={isOwn}
      />

      <div className="min-w-0 overflow-x-auto scrollbar-none">
        <Tabs value={view} onValueChange={changeView}>
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
                    view === tab.value
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

      {items.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <LibraryBigIcon />
            </EmptyMedia>
            <EmptyTitle>
              {isFiltered
                ? t("profile.libraryEmptyFiltered")
                : t("profile.libraryEmpty")}
            </EmptyTitle>
            <EmptyDescription>
              {isFiltered
                ? t("profile.libraryEmptyFilteredDescription")
                : t("profile.libraryEmptyDescription")}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="grid grid-cols-2 gap-x-4 gap-y-6 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
          {items.map((item) => (
            <LibraryCard
              key={item.title_id}
              item={item}
              coverUrl={
                item.title.has_cover ? titleCoverUrl(item.title, "mid") : null
              }
            />
          ))}
        </div>
      )}

      <Pagination page={page} pages={pages} onPageChange={changePage} />
    </div>
  );
}

function ProfileSkeleton() {
  return (
    <div className="flex w-full flex-col gap-5">
      <div className="flex items-center gap-4 rounded-xl p-4 ring-1 ring-foreground/10">
        <Skeleton className="size-16 rounded-full" />
        <div className="flex flex-1 flex-col gap-2">
          <Skeleton className="h-5 w-40" />
          <Skeleton className="h-3 w-28" />
        </div>
      </div>
      <div className="grid grid-cols-2 gap-x-4 gap-y-6 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
        {Array.from({ length: PAGE_SIZE }, (_, index) => (
          <div key={index} className="flex flex-col gap-2">
            <Skeleton className="aspect-2/3 w-full rounded-xl" />
            <Skeleton className="h-3 w-2/3" />
            <Skeleton className="h-4 w-5/6" />
          </div>
        ))}
      </div>
    </div>
  );
}

function NotFound({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <UserRoundIcon />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}
