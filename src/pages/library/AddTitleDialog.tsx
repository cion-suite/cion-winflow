import { type FormEvent, useEffect, useState } from "react";
import { SearchIcon } from "lucide-react";
import { Link, useNavigate } from "react-router";

import {
  sourceCoverUrl,
  useAddDiscoveredTitleMutation,
  useDiscoverSearchQuery,
  useErrorText,
  type DiscoverItem,
} from "@/api";
import { apiErrorKey, requiresSourceAccount } from "@/lib/apiErrors";
import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { CoverImage } from "@/ui/CoverImage";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui/shadcn/dialog";
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
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from "@/ui/shadcn/item";
import { Spinner } from "@/ui/shadcn/spinner";

const DEBOUNCE_MS = 300;

type AddTitleDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/**
 * Поиск идёт по всем включённым источникам. Добавление импортирует тайтл
 * на сервер, если его там ещё нет, и сразу открывает карточку.
 */
export function AddTitleDialog({ open, onOpenChange }: AddTitleDialogProps) {
  const t = useT();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const search = useDiscoverSearchQuery(debounced);
  const add = useAddDiscoveredTitleMutation();
  const errorText = useErrorText(add.error);

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(query.trim()), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [query]);

  function handleOpenChange(next: boolean): void {
    if (!next) {
      setQuery("");
      setDebounced("");
      add.reset();
    }
    onOpenChange(next);
  }

  function openTitle(titleId: number): void {
    handleOpenChange(false);
    void navigate(titlePath(titleId));
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const first = search.data?.items[0];
    if (first) handleAdd(first);
  }

  function handleAdd(item: DiscoverItem): void {
    if (item.in_library && item.title_id !== null) {
      openTitle(item.title_id);
      return;
    }
    add.mutate(
      { code: item.code, externalId: item.external_id },
      { onSuccess: (response) => openTitle(response.title_id) },
    );
  }

  const pendingId = add.isPending ? add.variables?.externalId : null;
  const items = search.data?.items ?? [];
  const sourceErrors = search.data?.errors ?? [];

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("library.addTitle")}</DialogTitle>
          <DialogDescription>{t("library.addTitleDescription")}</DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit}>
          <div className="relative">
            <SearchIcon
              aria-hidden
              className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              autoFocus
              value={query}
              placeholder={t("library.addSearchPlaceholder")}
              aria-label={t("library.addSearchPlaceholder")}
              className="pl-8"
              onChange={(event) => setQuery(event.target.value)}
            />
            {search.isFetching && (
              <Spinner
                className="absolute top-1/2 right-2.5 -translate-y-1/2 text-muted-foreground"
                aria-label={t("library.searching")}
              />
            )}
          </div>
        </form>

        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}

        <div className="max-h-[60vh] overflow-y-auto">
          {debounced === "" ? (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <SearchIcon />
                </EmptyMedia>
                <EmptyTitle>{t("library.addEmptyTitle")}</EmptyTitle>
                <EmptyDescription>{t("library.addEmptyDescription")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : search.isPending ? (
            <div className="flex items-center justify-center gap-2 p-6 text-sm text-muted-foreground">
              <Spinner aria-label={t("library.searching")} />
              {t("library.searching")}
            </div>
          ) : items.length === 0 ? (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <SearchIcon />
                </EmptyMedia>
                <EmptyTitle>{t("library.searchEmpty")}</EmptyTitle>
                <EmptyDescription>{t("library.addNoResults")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col gap-1">
              {items.map((item) => {
                const pending = pendingId === item.external_id;
                return (
                  <Item
                    key={`${item.code}:${item.external_id}`}
                    variant="outline"
                    size="sm"
                  >
                    {/* Крупная обложка: по ней проще узнать тайтл в выдаче */}
                    <div className="h-24 w-16 shrink-0 overflow-hidden rounded-md">
                      <CoverImage
                        src={
                          item.cover_ref
                            ? sourceCoverUrl(item.code, item.cover_ref)
                            : null
                        }
                        alt={item.name}
                        className="size-full"
                      />
                    </div>
                    <ItemContent>
                      <ItemTitle>{item.name}</ItemTitle>
                      <ItemDescription>
                        {item.source_name}
                        {item.year !== null ? ` · ${item.year}` : ""}
                        {item.title_kind !== null && (
                          <> · {t(`catalog.types.${item.title_kind}`)}</>
                        )}
                      </ItemDescription>
                    </ItemContent>
                    <ItemActions>
                      {item.status !== null && (
                        <Badge variant="outline">
                          {t(`title.statuses.${item.status}`)}
                        </Badge>
                      )}
                      {item.in_library && (
                        <Badge variant="secondary">{t("library.inLibrary")}</Badge>
                      )}
                      <Button
                        type="button"
                        size="sm"
                        disabled={add.isPending}
                        onClick={() => handleAdd(item)}
                      >
                        {pending && <Spinner data-icon="inline-start" />}
                        {item.in_library
                          ? t("library.open")
                          : t("library.add")}
                      </Button>
                    </ItemActions>
                  </Item>
                );
              })}
            </div>
          )}
        </div>

        {sourceErrors.length > 0 && (
          <div className="flex flex-col gap-1">
            {sourceErrors.map((sourceError) => {
              const key = apiErrorKey(sourceError.error_code);
              return (
                <p
                  key={`${sourceError.code}:${sourceError.message}`}
                  className="text-xs text-muted-foreground"
                >
                  {sourceError.source_name}: {key ? t(key) : sourceError.message}
                  {requiresSourceAccount(sourceError.error_code) && (
                    <>
                      {" "}
                      <Link className="underline" to="/settings">
                        {t("settingsPage.tabs.sources")}
                      </Link>
                    </>
                  )}
                </p>
              );
            })}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
