import { type FormEvent, useMemo, useState } from "react";

import {
  useAttachSourceMutation,
  useDetachSourceMutation,
  useErrorText,
  useRefreshTitleMutation,
  useSetPrimarySourceMutation,
  useSourceSearchMutation,
  useSourcesQuery,
  type TitleSource,
} from "@/api";
import type { MediaKind } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";

type Props = {
  titleId: number;
  titleKind: MediaKind;
  sources: TitleSource[];
  preferredSourceId: number | null;
};

export function AdminSourcePanel({ titleId, titleKind, sources, preferredSourceId }: Props) {
  const t = useT();
  const registry = useSourcesQuery();
  const search = useSourceSearchMutation();
  const attach = useAttachSourceMutation(titleId);
  const primary = useSetPrimarySourceMutation(titleId);
  const detach = useDetachSourceMutation(titleId);
  const refresh = useRefreshTitleMutation(titleId);

  const [selectedCode, setSelectedCode] = useState("");
  const [query, setQuery] = useState("");
  const [confirmDetach, setConfirmDetach] = useState<number | null>(null);

  const searchError = useErrorText(search.error);
  const attachError = useErrorText(attach.error);
  const primaryError = useErrorText(primary.error);
  const detachError = useErrorText(detach.error);
  const refreshError = useErrorText(refresh.error);

  const available = useMemo(
    () =>
      (registry.data?.sources ?? []).filter(
        (source) =>
          source.configured &&
          source.enabled &&
          source.kinds.includes(titleKind) &&
          source.capabilities.includes("search") &&
          source.capabilities.includes("chapters"),
      ),
    [registry.data, titleKind],
  );

  // Выбранный код не храним отдельно от списка: при смене тайтла/источников
  // он выводится из доступного списка, иначе select показывал бы пустоту.
  const code = available.some((source) => source.code === selectedCode)
    ? selectedCode
    : (available[0]?.code ?? "");
  const sourceOptions = available.map((source) => ({
    value: source.code,
    label: source.name,
  }));

  function handleSearch(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const value = query.trim();
    if (value === "" || code === "") return;
    search.mutate({ code, input: { query: value, kind: titleKind } });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("admin.titleSources")}</CardTitle>
        <CardDescription>{t("admin.titleSourcesHint")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {available.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("admin.noSearchSources")}</p>
        ) : (
          <form className="flex flex-col gap-2 sm:flex-row" onSubmit={handleSearch}>
            <Select
              items={sourceOptions}
              value={code}
              onValueChange={(value) => setSelectedCode(value ?? "")}
            >
              <SelectTrigger className="w-full sm:w-48" aria-label={t("admin.source")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {sourceOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input
              value={query}
              placeholder={t("admin.searchPlaceholder")}
              aria-label={t("admin.searchPlaceholder")}
              onChange={(event) => setQuery(event.target.value)}
            />
            <Button type="submit" variant="outline" disabled={search.isPending}>
              {t("admin.search")}
            </Button>
          </form>
        )}

        {searchError && <p role="alert" className="text-sm text-destructive">{searchError}</p>}

        {search.data && search.data.items.length === 0 && (
          <p className="text-sm text-muted-foreground">{t("admin.searchEmpty")}</p>
        )}

        {search.data && search.data.items.length > 0 && (
          <ul className="divide-y divide-border rounded-lg border border-border">
            {search.data.items.map((item) => (
              <li
                key={`${search.variables?.code}:${item.external_id}`}
                className="flex items-center justify-between gap-3 px-3 py-2"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">{item.name}</p>
                  <p className="truncate text-xs text-muted-foreground">
                    {item.external_id}
                    {item.year !== null ? ` · ${item.year}` : ""}
                  </p>
                </div>
                <Button
                  type="button"
                  size="sm"
                  disabled={attach.isPending}
                  onClick={() =>
                    attach.mutate({
                      code: search.variables?.code ?? code,
                      external_id: item.external_id,
                      kind: titleKind,
                    })
                  }
                >
                  {t("admin.attach")}
                </Button>
              </li>
            ))}
          </ul>
        )}

        {attachError && <p role="alert" className="text-sm text-destructive">{attachError}</p>}
        {attach.isSuccess && (
          <p className="text-sm text-muted-foreground">
            {t("admin.attached", { count: attach.data.imported_chapters })}
          </p>
        )}

        <div className="flex items-center justify-between gap-2">
          <h4 className="text-sm font-medium">{t("admin.attachedSources")}</h4>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={refresh.isPending || sources.length === 0}
            onClick={() => refresh.mutate()}
          >
            {t("admin.refresh")}
          </Button>
        </div>

        {refreshError && <p role="alert" className="text-sm text-destructive">{refreshError}</p>}
        {refresh.data && (
          <ul className="flex flex-col gap-1 text-xs text-muted-foreground">
            {refresh.data.entries.map((entry) => (
              <li key={entry.source_entry_id}>
                {entry.code}: {entry.status === "ok"
                  ? t("admin.refreshOk", { count: entry.imported_chapters })
                  : t("admin.refreshFailed", { message: entry.error ?? "" })}
              </li>
            ))}
          </ul>
        )}

        {sources.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("admin.noAttachedSources")}</p>
        ) : (
          <ul className="flex flex-col divide-y divide-border">
            {sources.map((source) => (
              <li key={source.source_entry_id} className="flex flex-col gap-2 py-2 first:pt-0">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium">{source.name}</span>
                    <span className="text-xs text-muted-foreground">{source.code}</span>
                    {source.is_primary && <Badge variant="secondary">{t("title.primarySource")}</Badge>}
                    {preferredSourceId === source.source_id && (
                      <Badge>{t("title.preferredSource")}</Badge>
                    )}
                    {!source.enabled && <Badge variant="outline">{t("title.sourceDisabled")}</Badge>}
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {t("title.sourceStats", {
                      chapters: source.chapter_count,
                      available: source.available_count,
                    })}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={source.is_primary || primary.isPending}
                    onClick={() => primary.mutate(source.source_entry_id)}
                  >
                    {t("admin.makePrimary")}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant={confirmDetach === source.source_entry_id ? "destructive" : "outline"}
                    disabled={detach.isPending}
                    onClick={() => {
                      if (confirmDetach === source.source_entry_id) {
                        detach.mutate(source.source_entry_id);
                        setConfirmDetach(null);
                      } else {
                        setConfirmDetach(source.source_entry_id);
                      }
                    }}
                  >
                    {confirmDetach === source.source_entry_id
                      ? t("admin.confirmDetach")
                      : t("admin.detach")}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}

        {primaryError && <p role="alert" className="text-sm text-destructive">{primaryError}</p>}
        {detachError && <p role="alert" className="text-sm text-destructive">{detachError}</p>}
      </CardContent>
    </Card>
  );
}
