import { Link } from "react-router";
import { Trash2Icon } from "lucide-react";
import { useState } from "react";

import {
  titleCoverUrl,
  useErrorText,
  useLibraryQuery,
  useRemoveLibraryMutation,
  useUpsertLibraryMutation,
  type LibraryItem,
} from "@/api";
import { LIBRARY_STATUSES, type LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { CoverImage } from "@/ui/CoverImage";
import { PageHeader } from "@/ui/PageHeader";
import { Pagination } from "@/ui/Pagination";
import { Select } from "@/ui/Select";
import { Badge } from "@/ui/shadcn/badge";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";
import { Skeleton } from "@/ui/shadcn/skeleton";

function LibraryRow({ item }: { item: LibraryItem }) {
  const t = useT();
  const upsert = useUpsertLibraryMutation(item.title_id);
  const remove = useRemoveLibraryMutation(item.title_id);
  const errorText = useErrorText(upsert.error ?? remove.error);
  const pending = upsert.isPending || remove.isPending;
  // Пока PUT в пути, select показывает выбор пользователя, а не старое значение
  const [status, setStatus] = useState<LibraryStatus>(item.status);
  const [score, setScore] = useState<number | null>(item.score);

  function revertToItem(): void {
    setStatus(item.status);
    setScore(item.score);
  }

  function save(nextStatus: LibraryStatus, nextScore: number | null): void {
    remove.reset();
    upsert.mutate(
      { status: nextStatus, score: nextScore },
      { onError: revertToItem },
    );
  }

  function changeStatus(next: LibraryStatus): void {
    setStatus(next);
    save(next, score);
  }

  function changeScore(next: number | null): void {
    setScore(next);
    save(status, next);
  }

  return (
    <Card size="sm">
      <div className="flex flex-wrap items-center gap-3 px-(--card-spacing)">
        <Link
          to={titlePath(item.title_id)}
          className="flex min-w-0 flex-1 items-center gap-3 outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          <CoverImage
            src={item.title.has_cover ? titleCoverUrl(item.title, "thumb") : null}
            alt={item.title.name}
            className="h-24 w-16 shrink-0 rounded-md"
          />
          <div className="flex min-w-0 flex-col gap-1">
            <span className="truncate text-sm font-medium">{item.title.name}</span>
            <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
              <Badge variant="outline">{t(`catalog.kinds.${item.title.kind}`)}</Badge>
              {item.title.year !== null && <span>{item.title.year}</span>}
            </div>
          </div>
        </Link>

        <div className="ml-auto flex items-center gap-2">
          <Select
            aria-label={t("library.status")}
            value={status}
            disabled={pending}
            className="w-40"
            onChange={(event) => changeStatus(event.target.value as LibraryStatus)}
          >
            {LIBRARY_STATUSES.map((value) => (
              <option key={value} value={value}>
                {t(`library.statuses.${value}`)}
              </option>
            ))}
          </Select>

          <Select
            aria-label={t("library.score")}
            value={score === null ? "" : String(score)}
            disabled={pending}
            className="w-24"
            onChange={(event) =>
              changeScore(event.target.value === "" ? null : Number(event.target.value))
            }
          >
            <option value="">{t("library.noScore")}</option>
            {Array.from({ length: 10 }, (_, index) => String(index + 1)).map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </Select>

          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={t("library.remove")}
            title={t("library.remove")}
            disabled={pending}
            onClick={() => {
              upsert.reset();
              remove.mutate();
            }}
          >
            <Trash2Icon aria-hidden />
          </Button>
        </div>

        {errorText && (
          <p role="alert" className="w-full text-sm text-destructive">
            {errorText}
          </p>
        )}
      </div>
    </Card>
  );
}

export function LibraryPage() {
  const t = useT();
  const [page, setPage] = useState(1);
  const query = useLibraryQuery({ page });
  const errorText = useErrorText(query.error);
  const items = query.data?.items ?? [];
  const total = query.data?.total ?? 0;
  const pageSize = query.data?.page_size ?? 50;
  const pages = Math.max(1, Math.ceil(total / pageSize), page);

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-4">
      <PageHeader title={t("library.title")} description={t("library.description")} />

      {query.isPending && (
        <div className="flex flex-col gap-2">
          {Array.from({ length: 4 }, (_, index) => (
            <Skeleton key={index} className="h-28 w-full rounded-xl" />
          ))}
        </div>
      )}

      {query.isError && (
        <Card>
          <CardContent className="flex flex-col items-start gap-2">
            <p role="alert" className="text-sm text-destructive">
              {errorText}
            </p>
            <Button type="button" variant="outline" onClick={() => void query.refetch()}>
              {t("retry")}
            </Button>
          </CardContent>
        </Card>
      )}

      {!query.isPending && !query.isError && items.length === 0 && total === 0 && (
        <Card>
          <CardHeader>
            <CardTitle>{t("library.empty")}</CardTitle>
            <CardDescription>{t("library.emptyDescription")}</CardDescription>
          </CardHeader>
          <CardContent>
            <Link className={buttonVariants()} to="/">
              {t("library.browseCatalog")}
            </Link>
          </CardContent>
        </Card>
      )}

      {!query.isPending && !query.isError && items.length === 0 && total > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>{t("library.emptyPage")}</CardTitle>
            <CardDescription>{t("library.emptyPageDescription")}</CardDescription>
          </CardHeader>
        </Card>
      )}

      {!query.isError && items.length > 0 && (
        <div className="flex flex-col gap-2">
          {items.map((item) => (
            <LibraryRow
              key={`${item.title_id}:${item.status}:${item.score ?? ""}`}
              item={item}
            />
          ))}
        </div>
      )}

      <Pagination page={page} pages={pages} onPageChange={setPage} />
    </div>
  );
}
