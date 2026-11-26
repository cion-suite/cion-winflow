import { useMemo, useState } from "react";
import {
  ArrowDownIcon,
  ArrowUpIcon,
  PlusIcon,
  RotateCcwIcon,
  ShieldIcon,
  WrenchIcon,
} from "lucide-react";
import { useNavigate } from "react-router";

import {
  formValuesToTitleInput,
  useAuth,
  useCreateTitleMutation,
  useErrorText,
  useProbeSourceMutation,
  useSaveSourcePrioritiesMutation,
  useSourcePrioritiesQuery,
  useSourcesQuery,
  useToggleSourceMutation,
  type SourcePriority,
  type SourceState,
} from "@/api";
import { emptyTitleFormValues, type TitleFormValues } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { PageHeader } from "@/ui/PageHeader";
import { SourceIcon } from "@/ui/SourceIcon";
import { TitleForm } from "@/ui/TitleForm";
import { Button } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/ui/shadcn/item";
import { Label } from "@/ui/shadcn/label";
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "@/ui/shadcn/popover";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Switch } from "@/ui/shadcn/switch";

function sourceDomain(source: SourceState): string {
  try {
    return new URL(source.site_url).host;
  } catch {
    return source.name;
  }
}

function SourceRow({
  source,
  index,
  total,
  pending,
  onMove,
}: {
  source: SourceState;
  index: number;
  total: number;
  pending: boolean;
  onMove: (delta: number) => void;
}) {
  const t = useT();
  const toggle = useToggleSourceMutation();
  const probe = useProbeSourceMutation();
  const [open, setOpen] = useState(false);
  const [baseUrl, setBaseUrl] = useState(
    source.base_url ?? source.default_base_url,
  );

  const toggleError = useErrorText(toggle.error);
  const probeError = useErrorText(probe.error);
  const rowToggleError =
    toggle.variables?.code === source.code ? toggleError : null;
  const rowProbeError =
    probe.variables === source.code ? probeError : null;
  const probed =
    probe.variables === source.code ? (probe.data ?? null) : null;
  const configured = source.configured;

  function saveUrl(url: string): void {
    toggle.mutate(
      { code: source.code, baseUrl: url.trim(), enabled: source.enabled },
      { onSuccess: () => setOpen(false) },
    );
  }

  return (
    <Item size="sm" className="rounded-none">
      <div className="flex shrink-0 items-center gap-0.5">
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={t("admin.moveUp")}
          disabled={index === 0 || pending}
          onClick={() => onMove(-1)}
        >
          <ArrowUpIcon aria-hidden />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={t("admin.moveDown")}
          disabled={index === total - 1 || pending}
          onClick={() => onMove(1)}
        >
          <ArrowDownIcon aria-hidden />
        </Button>
      </div>

      <ItemMedia className="size-8 rounded-md bg-muted p-1">
        <SourceIcon code={source.code} />
      </ItemMedia>

      <ItemContent>
        <ItemTitle>{sourceDomain(source)}</ItemTitle>
        <ItemDescription>
          {source.name}
          {!configured && ` · ${t("admin.notConfigured")}`}
        </ItemDescription>
      </ItemContent>

      <ItemActions>
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger
            render={
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={t("admin.editUrl")}
              />
            }
          >
            <WrenchIcon aria-hidden />
          </PopoverTrigger>
          <PopoverContent align="end" className="w-80 max-w-[calc(100vw-2rem)]">
            <PopoverHeader>
              <PopoverTitle>{source.name}</PopoverTitle>
              <PopoverDescription>{t("admin.baseUrlHint")}</PopoverDescription>
            </PopoverHeader>

            <div className="flex flex-col gap-2">
              <Label htmlFor={`source-url-${source.code}`}>
                {t("admin.baseUrl")}
              </Label>
              <Input
                id={`source-url-${source.code}`}
                value={baseUrl}
                placeholder={source.default_base_url}
                onChange={(event) => setBaseUrl(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t("admin.defaultUrl", {
                  url: source.default_base_url || "—",
                })}
              </p>

              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={baseUrl.trim() === "" || toggle.isPending}
                  onClick={() => saveUrl(baseUrl)}
                >
                  {t("admin.save")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={source.default_base_url === "" || toggle.isPending}
                  onClick={() => {
                    setBaseUrl(source.default_base_url);
                    saveUrl(source.default_base_url);
                  }}
                >
                  <RotateCcwIcon data-icon="inline-start" />
                  {t("admin.resetUrl")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={!configured || probe.isPending}
                  onClick={() => probe.mutate(source.code)}
                >
                  {t("admin.probe")}
                </Button>
              </div>

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
              {probed !== null && (
                <p className="text-xs text-muted-foreground">
                  {t("admin.probeResult", {
                    status: probed.status,
                    message: probed.message ?? "",
                  })}
                </p>
              )}
              {rowToggleError !== null && (
                <p role="alert" className="text-xs text-destructive">
                  {rowToggleError}
                </p>
              )}
              {rowProbeError !== null && (
                <p role="alert" className="text-xs text-destructive">
                  {rowProbeError}
                </p>
              )}
            </div>
          </PopoverContent>
        </Popover>

        <Switch
          checked={source.enabled}
          disabled={toggle.isPending}
          aria-label={
            source.enabled ? t("admin.disable") : t("admin.enable")
          }
          onCheckedChange={(checked) =>
            toggle.mutate({
              code: source.code,
              baseUrl: source.base_url ?? source.default_base_url,
              enabled: checked,
            })
          }
        />
      </ItemActions>
    </Item>
  );
}

function SourceColumn({
  lang,
  label,
  sources,
}: {
  lang: string;
  label: string;
  sources: SourceState[];
}) {
  const t = useT();
  const priorities = useSourcePrioritiesQuery(lang);
  const save = useSaveSourcePrioritiesMutation();
  const prioritiesError = useErrorText(priorities.error);
  const saveError = useErrorText(save.error);

  const serverOrder = useMemo(() => {
    const available = new Set(sources.map((source) => source.code));
    const ranked = (priorities.data?.sources ?? [])
      .map((source: SourcePriority) => source.code)
      .filter((code) => available.has(code));
    const rankedSet = new Set(ranked);
    // Без сохранённых приоритетов сохраняем порядок дескрипторов:
    // Remanga → RuMix → Mangamir, а не алфавит.
    const rest = sources
      .map((source) => source.code)
      .filter((code) => !rankedSet.has(code));
    return [...ranked, ...rest];
  }, [sources, priorities.data]);

  // Оптимистичный порядок на время запроса. Сервер отвечает тем же
  // списком, после refetch он становится источником истины.
  const [pendingOrder, setPendingOrder] = useState<string[] | null>(null);
  const order = pendingOrder ?? serverOrder;

  const byCode = useMemo(
    () => new Map(sources.map((source) => [source.code, source])),
    [sources],
  );

  function move(code: string, delta: number): void {
    const index = order.indexOf(code);
    const target = index + delta;
    if (index < 0 || target < 0 || target >= order.length) return;

    const next = [...order];
    [next[index], next[target]] = [next[target], next[index]];
    setPendingOrder(next);
    save.mutate(
      { lang, sourceCodes: next },
      {
        onSuccess: () => {
          void priorities.refetch().finally(() => setPendingOrder(null));
        },
        onError: () => setPendingOrder(null),
      },
    );
  }

  return (
    <section className="flex min-w-0 flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-medium">{label}</h3>
        <span className="text-xs text-muted-foreground">
          {t("admin.priorityHint")}
        </span>
      </div>

      {priorities.isPending && <Skeleton className="h-32 w-full" />}

      {priorities.isError && (
        <div className="flex flex-col items-start gap-2">
          <p role="alert" className="text-sm text-destructive">
            {prioritiesError}
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void priorities.refetch()}
          >
            {t("retry")}
          </Button>
        </div>
      )}

      {!priorities.isPending && !priorities.isError && order.length === 0 && (
        <p className="rounded-xl border border-dashed p-6 text-sm text-muted-foreground">
          {t("admin.emptyColumn")}
        </p>
      )}

      {!priorities.isPending && order.length > 0 && (
        <ItemGroup className="gap-0 divide-y overflow-hidden rounded-xl border">
          {order.map((code, index) => {
            const source = byCode.get(code);
            if (source === undefined) return null;
            return (
              <SourceRow
                key={`${code}:${source.base_url ?? ""}`}
                source={source}
                index={index}
                total={order.length}
                pending={save.isPending || priorities.isFetching}
                onMove={(delta) => move(code, delta)}
              />
            );
          })}
        </ItemGroup>
      )}

      {save.isError && (
        <p role="alert" className="text-sm text-destructive">
          {saveError}
        </p>
      )}
    </section>
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
  const sources = useSourcesQuery();
  const sourcesError = useErrorText(sources.error);

  const allSources = sources.data?.sources;
  const ruSources = useMemo(
    () =>
      (allSources ?? []).filter(
        (source) =>
          source.lang === "ru" &&
          (source.default_base_url !== "" || source.configured),
      ),
    [allSources],
  );
  const enSources = useMemo(
    () =>
      (allSources ?? []).filter(
        (source) =>
          source.lang === "en" &&
          (source.default_base_url !== "" || source.configured),
      ),
    [allSources],
  );

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <PageHeader title={t("admin.title")} description={t("admin.description")} />

      <ManualTitleCard />

      <Card>
        <CardHeader>
          <CardTitle>{t("admin.sources")}</CardTitle>
          <CardDescription>{t("admin.sourcesHint")}</CardDescription>
        </CardHeader>
        <CardContent>
          {sources.isPending && <Skeleton className="h-40 w-full" />}
          {sources.isError && (
            <div className="flex flex-col items-start gap-2">
              <p role="alert" className="text-sm text-destructive">
                {sourcesError}
              </p>
              <Button
                type="button"
                variant="outline"
                onClick={() => void sources.refetch()}
              >
                {t("retry")}
              </Button>
            </div>
          )}
          {sources.data && (
            <div className="grid gap-6 lg:grid-cols-2">
              <SourceColumn
                lang="ru"
                label={t("admin.russian")}
                sources={ruSources}
              />
              <SourceColumn
                lang="en"
                label={t("admin.english")}
                sources={enSources}
              />
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
