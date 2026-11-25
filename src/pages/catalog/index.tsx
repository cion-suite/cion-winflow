import { type FormEvent, useState } from "react";
import { Link } from "react-router";
import { PlusIcon, SearchIcon, StarIcon } from "lucide-react";

import {
  formValuesToTitleInput,
  titleCoverUrl,
  useAuth,
  useCatalogQuery,
  useCreateTitleMutation,
  useErrorText,
  type TitleSummary,
} from "@/api";
import {
  emptyTitleFormValues,
  MEDIA_KINDS,
  type MediaKind,
  type TitleFormValues,
} from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { CoverImage } from "@/ui/CoverImage";
import { PageHeader } from "@/ui/PageHeader";
import { Pagination } from "@/ui/Pagination";
import { Select } from "@/ui/Select";
import { TitleForm } from "@/ui/TitleForm";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";
import { Skeleton } from "@/ui/shadcn/skeleton";

function CatalogCard({ item }: { item: TitleSummary }) {
  const t = useT();

  return (
    <Link
      to={titlePath(item.id)}
      className="group flex flex-col gap-2 rounded-xl outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <div className="overflow-hidden rounded-xl ring-1 ring-foreground/10">
        <CoverImage
          src={item.has_cover ? titleCoverUrl(item, "thumb") : null}
          alt={item.name}
          className="aspect-2/3 w-full transition-transform group-hover:scale-[1.02]"
        />
      </div>
      <div className="flex flex-col gap-1">
        <span className="line-clamp-2 text-sm font-medium">{item.name}</span>
        <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          <Badge variant="outline">{t(`catalog.kinds.${item.kind}`)}</Badge>
          {item.year !== null && <span>{item.year}</span>}
          {item.rating !== null && (
            <span className="inline-flex items-center gap-0.5">
              <StarIcon className="size-3" aria-hidden />
              {item.rating.toFixed(1)}
            </span>
          )}
        </div>
      </div>
    </Link>
  );
}

export function CatalogPage() {
  const t = useT();
  const { user } = useAuth();
  const [search, setSearch] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [kind, setKind] = useState<"" | MediaKind>("");
  const [page, setPage] = useState(1);
  const [creating, setCreating] = useState(false);

  const catalog = useCatalogQuery({
    q: submitted || undefined,
    kind: kind || undefined,
    page,
  });
  const createTitle = useCreateTitleMutation();

  const catalogError = useErrorText(catalog.error);
  const createError = useErrorText(createTitle.error);
  const isAdmin = user?.role === "admin";

  const total = catalog.data?.total ?? 0;
  const pageSize = catalog.data?.page_size ?? 50;
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const items = catalog.data?.items ?? [];
  const filtered = submitted !== "" || kind !== "";

  function handleSearch(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    setSubmitted(search.trim());
    setPage(1);
  }

  function handleCreate(values: TitleFormValues): void {
    createTitle.mutate(formValuesToTitleInput(values), {
      onSuccess: () => setCreating(false),
    });
  }

  function toggleCreating(): void {
    createTitle.reset();
    setCreating((value) => !value);
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-4">
      <PageHeader
        title={t("catalog.title")}
        description={t("catalog.description")}
        actions={
          isAdmin && (
            <Button type="button" onClick={toggleCreating}>
              <PlusIcon aria-hidden />
              {t("catalog.addTitle")}
            </Button>
          )
        }
      />

      {creating && isAdmin && (
        <Card>
          <CardHeader>
            <CardTitle>{t("catalog.addTitle")}</CardTitle>
            <CardDescription>{t("catalog.addTitleHint")}</CardDescription>
          </CardHeader>
          <CardContent>
            <TitleForm
              initial={emptyTitleFormValues()}
              submitLabel={t("catalog.create")}
              isPending={createTitle.isPending}
              errorText={createError}
              onSubmit={handleCreate}
              onCancel={() => {
                createTitle.reset();
                setCreating(false);
              }}
            />
          </CardContent>
        </Card>
      )}

      <form className="flex flex-col gap-2 sm:flex-row" onSubmit={handleSearch}>
        <Input
          name="q"
          value={search}
          placeholder={t("catalog.searchPlaceholder")}
          aria-label={t("catalog.searchPlaceholder")}
          className="sm:max-w-sm"
          onChange={(event) => setSearch(event.target.value)}
        />
        <Select
          name="kind"
          value={kind}
          aria-label={t("title.kind")}
          className="sm:w-40"
          onChange={(event) => {
            setKind(event.target.value as "" | MediaKind);
            setPage(1);
          }}
        >
          <option value="">{t("catalog.allKinds")}</option>
          {MEDIA_KINDS.map((value) => (
            <option key={value} value={value}>
              {t(`catalog.kinds.${value}`)}
            </option>
          ))}
        </Select>
        <Button type="submit" variant="outline">
          <SearchIcon aria-hidden />
          {t("catalog.search")}
        </Button>
      </form>

      {catalog.isPending && (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
          {Array.from({ length: 10 }, (_, index) => (
            <div key={index} className="flex flex-col gap-2">
              <Skeleton className="aspect-2/3 w-full rounded-xl" />
              <Skeleton className="h-4 w-3/4" />
            </div>
          ))}
        </div>
      )}

      {catalog.isError && (
        <Card>
          <CardContent className="flex flex-col items-start gap-2">
            <p role="alert" className="text-sm text-destructive">
              {catalogError}
            </p>
            <Button type="button" variant="outline" onClick={() => void catalog.refetch()}>
              {t("retry")}
            </Button>
          </CardContent>
        </Card>
      )}

      {!catalog.isPending && !catalog.isError && items.length === 0 && (
        <Card>
          <CardHeader>
            <CardTitle>{filtered ? t("catalog.noResults") : t("catalog.empty")}</CardTitle>
            <CardDescription>
              {filtered ? t("catalog.noResultsDescription") : t("catalog.emptyDescription")}
            </CardDescription>
          </CardHeader>
        </Card>
      )}

      {!catalog.isError && items.length > 0 && (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
          {items.map((item) => (
            <CatalogCard key={item.id} item={item} />
          ))}
        </div>
      )}

      <Pagination page={page} pages={pages} onPageChange={setPage} />
    </div>
  );
}
