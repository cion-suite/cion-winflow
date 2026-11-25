import { type FormEvent, useState } from "react";
import { ArrowDownIcon, ArrowUpIcon, PlusIcon, ShieldIcon } from "lucide-react";
import { Link, useNavigate } from "react-router";

import {
  formValuesToTitleInput,
  useAuth,
  useCreateTitleMutation,
  useErrorText,
  useImportSourceMutation,
  useProbeSourceMutation,
  useSaveSourcePrioritiesMutation,
  useSourcePrioritiesQuery,
  useSourceSearchMutation,
  useSourcesQuery,
  useToggleSourceMutation,
  type SourcePriority,
  type SourceState,
} from "@/api";
import {
  emptyTitleFormValues,
  MEDIA_KINDS,
  type MediaKind,
  type TitleFormValues,
} from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { PageHeader } from "@/ui/PageHeader";
import { Select } from "@/ui/Select";
import { TitleForm } from "@/ui/TitleForm";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";
import { Skeleton } from "@/ui/shadcn/skeleton";

function SourceRow({ source }: { source: SourceState }) {
  const t = useT();
  const toggle = useToggleSourceMutation();
  const probe = useProbeSourceMutation();
  const [baseUrl, setBaseUrl] = useState(source.base_url ?? "");
  const [editing, setEditing] = useState(false);
  const showEditor = !source.configured || editing;
  const toggleError = useErrorText(toggle.error);
  const probeError = useErrorText(probe.error);
  const probed = probe.variables === source.code ? probe.data : null;

  return (
    <div className="flex flex-col gap-2 border-b border-border py-3 last:border-b-0 last:pb-0">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium">{source.name}</span>
          <span className="text-xs text-muted-foreground">{source.code}</span>
          <Badge variant={source.enabled ? "secondary" : "outline"}>
            {source.enabled ? t("admin.enabled") : t("admin.disabled")}
          </Badge>
          {!source.configured && <Badge variant="outline">{t("admin.notConfigured")}</Badge>}
        </div>
        {source.configured && (
          <div className="flex items-center gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={probe.isPending}
              onClick={() => probe.mutate(source.code)}
            >
              {t("admin.probe")}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={toggle.isPending}
              onClick={() =>
                toggle.mutate({
                  code: source.code,
                  baseUrl: source.base_url ?? null,
                  enabled: !source.enabled,
                })
              }
            >
              {source.enabled ? t("admin.disable") : t("admin.enable")}
            </Button>
          </div>
        )}
      </div>

      {source.configured && !editing && source.base_url !== null && (
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-xs text-muted-foreground">{source.base_url}</p>
          <Button type="button" size="sm" variant="outline" onClick={() => setEditing(true)}>
            {t("admin.editUrl")}
          </Button>
        </div>
      )}

      {showEditor && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-col gap-2 sm:flex-row">
            <Input
              value={baseUrl}
              placeholder={t("admin.baseUrlPlaceholder")}
              aria-label={t("admin.baseUrl")}
              onChange={(event) => setBaseUrl(event.target.value)}
            />
            <Button
              type="button"
              variant="outline"
              disabled={baseUrl.trim() === "" || toggle.isPending}
              onClick={() =>
                toggle.mutate(
                  {
                    code: source.code,
                    baseUrl: baseUrl.trim(),
                    enabled: source.configured ? source.enabled : true,
                  },
                  { onSuccess: () => setEditing(false) },
                )
              }
            >
              {t("admin.save")}
            </Button>
            {source.configured && (
              <Button
                type="button"
                variant="outline"
                disabled={toggle.isPending}
                onClick={() => {
                  setBaseUrl(source.base_url ?? "");
                  setEditing(false);
                }}
              >
                {t("title.cancel")}
              </Button>
            )}
          </div>
          <p className="text-xs text-muted-foreground">{t("admin.baseUrlHint")}</p>
        </div>
      )}
      {source.config_error !== null && (
        <p className="text-xs text-destructive">
          {t("admin.configError", { message: source.config_error })}
        </p>
      )}
      {source.last_error !== null && (
        <p className="text-xs text-destructive">
          {t("admin.lastError", { message: source.last_error })}
        </p>
      )}
      {probed && (
        <p className="text-xs text-muted-foreground">
          {t("admin.probeResult", {
            status: probed.status,
            message: probed.message ?? "",
          })}
        </p>
      )}
      {toggleError && (
        <p role="alert" className="text-xs text-destructive">
          {toggleError}
        </p>
      )}
      {probeError && (
        <p role="alert" className="text-xs text-destructive">
          {probeError}
        </p>
      )}
    </div>
  );
}

function ImportFromSource({ sources }: { sources: SourceState[] }) {
  const t = useT();
  const search = useSourceSearchMutation();
  const importTitle = useImportSourceMutation();
  const [code, setCode] = useState("");
  const [kind, setKind] = useState<MediaKind>("manga");
  const [query, setQuery] = useState("");

  const searchError = useErrorText(search.error);
  const importError = useErrorText(importTitle.error);

  const available = sources.filter(
    (source) =>
      source.configured &&
      source.enabled &&
      source.capabilities.includes("search") &&
      source.capabilities.includes("chapters"),
  );
  const selected = available.find((source) => source.code === code) ?? available[0];
  const effectiveKind = selected?.kinds.includes(kind)
    ? kind
    : (selected?.default_kind ?? "manga");

  function handleSearch(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const value = query.trim();
    if (!selected || value === "") return;
    search.mutate({
      code: selected.code,
      input: { query: value, kind: effectiveKind },
    });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("admin.importTitle")}</CardTitle>
        <CardDescription>{t("admin.importHint")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {available.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("admin.noImportSources")}</p>
        ) : (
          <form className="flex flex-col gap-2 sm:flex-row" onSubmit={handleSearch}>
            <Select
              className="sm:w-44"
              value={selected?.code ?? ""}
              aria-label={t("admin.source")}
              onChange={(event) => setCode(event.target.value)}
            >
              {available.map((source) => (
                <option key={source.code} value={source.code}>
                  {source.name}
                </option>
              ))}
            </Select>
            {selected && selected.kinds.length > 1 && (
              <Select
                className="sm:w-36"
                value={effectiveKind}
                aria-label={t("title.kind")}
                onChange={(event) => setKind(event.target.value as MediaKind)}
              >
                {selected.kinds.map((value) => (
                  <option key={value} value={value}>
                    {t(`catalog.kinds.${value}`)}
                  </option>
                ))}
              </Select>
            )}
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

        {searchError && (
          <p role="alert" className="text-sm text-destructive">
            {searchError}
          </p>
        )}
        {importError && (
          <p role="alert" className="text-sm text-destructive">
            {importError}
          </p>
        )}

        {importTitle.data && (
          <p className="text-sm">
            {t("admin.imported", { count: importTitle.data.imported_chapters })}{" "}
            <Link className="underline" to={titlePath(importTitle.data.title_id)}>
              {t("admin.openImported")}
            </Link>
          </p>
        )}

        {search.data && search.data.items.length === 0 && (
          <p className="text-sm text-muted-foreground">{t("admin.importEmpty")}</p>
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
                  disabled={importTitle.isPending}
                  onClick={() =>
                    importTitle.mutate({
                      code: search.variables?.code ?? selected?.code ?? "",
                      externalId: item.external_id,
                      kind: search.variables?.input.kind ?? effectiveKind,
                    })
                  }
                >
                  {t("admin.importButton")}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function ManualTitleCard() {
  const t = useT();
  const navigate = useNavigate();
  const create = useCreateTitleMutation();
  const [creating, setCreating] = useState(false);
  const errorText = useErrorText(create.error);

  function handleCreate(values: TitleFormValues): void {
    create.mutate(formValuesToTitleInput(values), {
      onSuccess: (payload) => void navigate(titlePath(payload.title.id)),
    });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("admin.manualTitle")}</CardTitle>
        <CardDescription>{t("admin.manualTitleHint")}</CardDescription>
      </CardHeader>
      <CardContent>
        {creating ? (
          <TitleForm
            initial={emptyTitleFormValues()}
            submitLabel={t("admin.createTitle")}
            isPending={create.isPending}
            errorText={errorText}
            onSubmit={handleCreate}
            onCancel={() => {
              create.reset();
              setCreating(false);
            }}
          />
        ) : (
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              create.reset();
              setCreating(true);
            }}
          >
            <PlusIcon data-icon="inline-start" />
            {t("admin.manualTitle")}
          </Button>
        )}
      </CardContent>
    </Card>
  );
}

function PrioritiesEditor({
  sources,
  isPending,
  isSaved,
  errorText,
  onSave,
}: {
  sources: SourcePriority[];
  isPending: boolean;
  isSaved: boolean;
  errorText: string | null;
  onSave: (codes: string[]) => void;
}) {
  const t = useT();
  const [order, setOrder] = useState<string[]>(sources.map((source) => source.code));
  const [dirty, setDirty] = useState(false);
  const byCode = new Map(sources.map((source) => [source.code, source]));

  function move(code: string, delta: number): void {
    const index = order.indexOf(code);
    const target = index + delta;
    if (index < 0 || target < 0 || target >= order.length) return;
    setDirty(true);
    setOrder((current) => {
      const next = [...current];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
  }

  if (sources.length === 0) {
    return <p className="text-sm text-muted-foreground">{t("admin.noSources")}</p>;
  }

  return (
    <>
      <ol className="flex flex-col divide-y divide-border">
        {order.map((code, index) => {
          const source = byCode.get(code);
          return (
            <li key={code} className="flex items-center justify-between gap-3 py-2">
              <div className="flex min-w-0 items-center gap-2">
                <span className="w-6 text-sm text-muted-foreground">{index + 1}</span>
                <span className="truncate text-sm font-medium">{source?.name ?? code}</span>
                <span className="text-xs text-muted-foreground">{code}</span>
                {source && !source.enabled && (
                  <Badge variant="outline">{t("admin.disabled")}</Badge>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  aria-label={t("admin.moveUp")}
                  disabled={index === 0}
                  onClick={() => move(code, -1)}
                >
                  <ArrowUpIcon aria-hidden />
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  aria-label={t("admin.moveDown")}
                  disabled={index === order.length - 1}
                  onClick={() => move(code, 1)}
                >
                  <ArrowDownIcon aria-hidden />
                </Button>
              </div>
            </li>
          );
        })}
      </ol>

      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}
      {isSaved && !dirty && (
        <p className="text-sm text-muted-foreground">{t("admin.saved")}</p>
      )}

      <div>
        <Button type="button" disabled={isPending} onClick={() => onSave(order)}>
          {t("admin.save")}
        </Button>
      </div>
    </>
  );
}

/** Обёртка без админских запросов: обычный пользователь не должен их слать. */
export function AdminPage() {
  const t = useT();
  const { user } = useAuth();

  if (user?.role !== "admin") {
    return (
      <Card>
        <CardContent className="flex items-center gap-2 py-6 text-sm text-muted-foreground">
          <ShieldIcon aria-hidden />
          {t("admin.forbidden")}
        </CardContent>
      </Card>
    );
  }

  return <AdminContent />;
}

function AdminContent() {
  const t = useT();
  const [kind, setKind] = useState<MediaKind>("manga");
  const [lang, setLang] = useState("ru");

  const sources = useSourcesQuery();
  const priorities = useSourcePrioritiesQuery(kind, lang);
  const save = useSaveSourcePrioritiesMutation();
  const prioritiesError = useErrorText(priorities.error);
  const saveError = useErrorText(save.error);
  const sourcesError = useErrorText(sources.error);

  const serverCodes = priorities.data?.sources.map((source) => source.code).join(",") ?? "";

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <PageHeader title={t("admin.title")} description={t("admin.description")} />

      <ImportFromSource sources={sources.data?.sources ?? []} />

      <ManualTitleCard />

      <Card>
        <CardHeader>
          <CardTitle>{t("admin.sources")}</CardTitle>
          <CardDescription>{t("admin.sourcesHint")}</CardDescription>
        </CardHeader>
        <CardContent>
          {sources.isPending && <Skeleton className="h-24 w-full" />}
          {sources.isError && (
            <div className="flex flex-col items-start gap-2">
              <p role="alert" className="text-sm text-destructive">
                {sourcesError}
              </p>
              <Button type="button" variant="outline" onClick={() => void sources.refetch()}>
                {t("retry")}
              </Button>
            </div>
          )}
          {sources.data?.sources.map((source) => (
            <SourceRow key={source.code} source={source} />
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("admin.priorities")}</CardTitle>
          <CardDescription>{t("admin.prioritiesHint")}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="grid gap-2 sm:grid-cols-2">
            <label className="flex flex-col gap-1.5 text-sm">
              <span className="font-medium">{t("title.kind")}</span>
              <Select value={kind} onChange={(event) => setKind(event.target.value as MediaKind)}>
                {MEDIA_KINDS.map((value) => (
                  <option key={value} value={value}>
                    {t(`catalog.kinds.${value}`)}
                  </option>
                ))}
              </Select>
            </label>
            <label className="flex flex-col gap-1.5 text-sm">
              <span className="font-medium">{t("admin.lang")}</span>
              <Input
                value={lang}
                maxLength={8}
                placeholder="ru"
                onChange={(event) => setLang(event.target.value)}
              />
              <span className="text-xs text-muted-foreground">{t("admin.langHint")}</span>
            </label>
          </div>

          {priorities.isPending && <Skeleton className="h-32 w-full" />}
          {priorities.isError && (
            <div className="flex flex-col items-start gap-2">
              <p role="alert" className="text-sm text-destructive">
                {prioritiesError}
              </p>
              <Button type="button" variant="outline" onClick={() => void priorities.refetch()}>
                {t("retry")}
              </Button>
            </div>
          )}

          {priorities.data && (
            <PrioritiesEditor
              key={`${kind}:${lang}:${serverCodes}`}
              sources={priorities.data.sources}
              isPending={save.isPending}
              isSaved={save.isSuccess}
              errorText={saveError}
              onSave={(codes) =>
                save.mutate({ kind, lang: lang.trim(), sourceCodes: codes })
              }
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
