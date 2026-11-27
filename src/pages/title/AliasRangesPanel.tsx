import { useMemo, useState } from "react";
import {
  AlertTriangleIcon,
  CheckIcon,
  ChevronDownIcon,
  Link2Icon,
  PlusIcon,
  ScanSearchIcon,
  Trash2Icon,
} from "lucide-react";

import {
  useAliasRangesQuery,
  useDeleteAliasMutation,
  useDeleteAliasRangeMutation,
  useErrorText,
  type AliasRange,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { SourceIcon } from "@/ui/SourceIcon";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui/shadcn/alert-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/ui/shadcn/alert";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/ui/shadcn/empty";
import {
  Collapsible,
  CollapsibleContent,
} from "@/ui/shadcn/collapsible";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Spinner } from "@/ui/shadcn/spinner";
import { AliasRangeDialog } from "./AliasRangeDialog";
import { AliasScanDialog } from "./AliasScanDialog";
import {
  collectVariants,
  findCanonicalSide,
  formatChapterLabel,
  type SourceVariants,
} from "./aliases";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  preferredSourceId: number | null;
  /** Язык просмотра: панель показывает связки источников этого языка. */
  lang: string | null;
};

function canonicalSourceOf(
  range: AliasRange,
  sourceVariants: readonly SourceVariants[],
): { code: string; name: string } | null {
  const side = findCanonicalSide(
    sourceVariants,
    range.canonical.chapter_key,
    range.canonical.chapter_id,
  );
  return side === undefined
    ? null
    : { code: side.source.code, name: side.source.name };
}

function statusLabel(
  status: AliasRange["status"],
  t: ReturnType<typeof useT>,
): string | null {
  if (status === "needs_review") return t("title.rangeStatusNeedsReview");
  if (status === "broken") return t("title.rangeStatusBroken");
  return null;
}

function aliasChapterLabel(
  key: string,
  chapters: readonly TitleChapter[],
  t: ReturnType<typeof useT>,
): string {
  const chapter = chapters.find((item) => item.chapter_key === key);
  if (chapter !== undefined) {
    return formatChapterLabel(
      { volume: chapter.volume, numberText: chapter.number_text },
      t,
    );
  }
  for (const item of chapters) {
    const variant = item.variants.find(
      (candidate) => candidate.chapter_key === key,
    );
    if (variant !== undefined) {
      return formatChapterLabel(
        { volume: variant.volume, numberText: variant.number_text },
        t,
      );
    }
  }
  return key;
}

function RangeRow({
  range,
  sourceVariants,
  onEdit,
  onDelete,
}: {
  range: AliasRange;
  sourceVariants: readonly SourceVariants[];
  onEdit: (range: AliasRange) => void;
  onDelete: (range: AliasRange) => void;
}) {
  const t = useT();
  const canonical = canonicalSourceOf(range, sourceVariants);
  const status = statusLabel(range.status, t);
  const anchorLabel = formatChapterLabel(
    { volume: range.anchor.volume, numberText: range.anchor.number_text },
    t,
  );
  const canonicalLabel = formatChapterLabel(
    { volume: range.canonical.volume, numberText: range.canonical.number_text },
    t,
  );

  const hasNotes =
    status !== null ||
    range.mapping_mode === "number_only" ||
    range.warnings.includes("anchor_missing") ||
    range.warnings.includes("canonical_missing");

  return (
    <div className="flex flex-col gap-2 rounded-xl border p-3">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <ItemSource code={range.source_code} />
          <div className="min-w-0">
            <div
              className="truncate text-sm font-medium"
              title={range.source_name}
            >
              {range.source_name}
            </div>
            <div
              className="line-clamp-2 text-xs leading-tight text-muted-foreground"
              title={anchorLabel}
            >
              {anchorLabel}
            </div>
          </div>
        </div>

        <div className="flex shrink-0 flex-col items-center gap-0.5 text-center">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={t("title.editAliasRange")}
            onClick={() => onEdit(range)}
          >
            <Link2Icon aria-hidden />
          </Button>
          <span className="whitespace-nowrap text-xs leading-tight text-muted-foreground">
            {t("title.rangeCoverageLinked", { linked: range.coverage.linked })}
          </span>
          {range.coverage.pending > 0 && (
            <span className="whitespace-nowrap text-xs leading-tight text-muted-foreground">
              {t("title.rangeCoveragePending", {
                pending: range.coverage.pending,
              })}
            </span>
          )}
        </div>

        <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
          <div className="min-w-0 text-right">
            <div
              className="truncate text-sm font-medium"
              title={canonical?.name ?? t("title.canonicalChapter")}
            >
              {canonical?.name ?? t("title.canonicalChapter")}
            </div>
            <div
              className="line-clamp-2 text-xs leading-tight text-muted-foreground"
              title={canonicalLabel}
            >
              {canonicalLabel}
            </div>
          </div>
          {canonical === null ? (
            <span className="size-7 shrink-0 rounded-md bg-muted" aria-hidden />
          ) : (
            <ItemSource code={canonical.code} />
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={t("title.rangeDelete")}
            onClick={() => onDelete(range)}
          >
            <Trash2Icon aria-hidden />
          </Button>
        </div>
      </div>

      {hasNotes && (
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          {status !== null && <Badge variant="outline">{status}</Badge>}
          {range.mapping_mode === "number_only" && (
            <Badge variant="outline">
              {t("title.rangeWarningNumberOnly")}
            </Badge>
          )}
          {range.warnings.includes("anchor_missing") && (
            <span className="text-destructive">
              {t("title.rangeWarningAnchorMissing")}
            </span>
          )}
          {range.warnings.includes("canonical_missing") && (
            <span className="text-destructive">
              {t("title.rangeWarningCanonicalMissing")}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function ItemSource({ code }: { code: string }) {
  return (
    <span className="grid size-7 shrink-0 place-items-center rounded-md bg-muted p-1">
      <SourceIcon code={code} />
    </span>
  );
}

export function AliasRangesPanel({
  titleId,
  sources,
  chapters,
  preferredSourceId,
  lang,
}: Props) {
  const t = useT();
  const query = useAliasRangesQuery(titleId);
  const queryError = useErrorText(query.error);
  const remove = useDeleteAliasRangeMutation(titleId);
  const removeError = useErrorText(remove.error);
  const removeAlias = useDeleteAliasMutation(titleId);
  const removeAliasError = useErrorText(removeAlias.error);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<AliasRange | null>(null);
  const [scanOpen, setScanOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<AliasRange | null>(null);
  const [manualOpen, setManualOpen] = useState(false);
  const [confirmingAlias, setConfirmingAlias] = useState<string | null>(null);

  const ranges = useMemo(() => query.data?.ranges ?? [], [query.data]);
  const manualAliases = query.data?.manual_aliases ?? [];
  // Для отображения канонической стороны нужны все источники: связка может
  // быть создана до появления языка просмотра.
  const sourceVariants = useMemo(
    () => collectVariants(chapters, sources, preferredSourceId),
    [chapters, preferredSourceId, sources],
  );
  // Связки и диалоги ограничены выбранным языком: настройки одного перевода
  // не должны смешиваться с другим.
  const langByCode = useMemo(
    () => new Map(sources.map((source) => [source.code, source.lang])),
    [sources],
  );
  const scopedSources = useMemo(
    () =>
      lang === null ? sources : sources.filter((source) => source.lang === lang),
    [lang, sources],
  );
  const visibleRanges = useMemo(
    () =>
      lang === null
        ? ranges
        : ranges.filter((range) => langByCode.get(range.source_code) === lang),
    [lang, langByCode, ranges],
  );
  const sourceCount = sourceVariants.filter(
    (item) => lang === null || item.source.lang === lang,
  ).length;
  // Без загруженных связок скан предложит уже существующие: сервер отклонит
  // пачку целиком, поэтому ждём ответ и не сканируем после ошибки.
  const canScan =
    sourceCount >= 2 && !query.isPending && !query.isError;
  const canCreate = sourceCount >= 2;

  function openCreate(): void {
    setEditing(null);
    setDialogOpen(true);
  }

  function openEdit(range: AliasRange): void {
    setEditing(range);
    setDialogOpen(true);
  }

  function confirmDelete(): void {
    if (deleteTarget === null) return;
    remove.mutate(deleteTarget.id, {
      onSuccess: () => setDeleteTarget(null),
    });
  }

  function deleteManual(aliasKey: string): void {
    if (confirmingAlias === aliasKey) {
      removeAlias.mutate(aliasKey, {
        onSuccess: () => setConfirmingAlias(null),
      });
      return;
    }
    setConfirmingAlias(aliasKey);
  }

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 className="text-sm font-medium">{t("title.aliasRanges")}</h2>
          <p className="max-w-prose text-sm text-muted-foreground">
            {t("title.aliasRangesHint")}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!canScan}
            onClick={() => setScanOpen(true)}
          >
            <ScanSearchIcon data-icon="inline-start" />
            {t("title.findAliasRanges")}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!canCreate}
            onClick={openCreate}
          >
            <PlusIcon data-icon="inline-start" />
            {t("title.addAliasRange")}
          </Button>
        </div>
      </div>

      {(queryError ?? removeError ?? removeAliasError) !== null && (
        <Alert variant="destructive">
          <AlertTriangleIcon aria-hidden />
          <AlertTitle>{t("errors.unknown")}</AlertTitle>
          <AlertDescription>
            {queryError ?? removeError ?? removeAliasError}
          </AlertDescription>
        </Alert>
      )}

      {query.isPending ? (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-20 w-full rounded-xl" />
          <Skeleton className="h-20 w-full rounded-xl" />
        </div>
      ) : visibleRanges.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Link2Icon />
            </EmptyMedia>
            <EmptyTitle>{t("title.aliasesEmptyTitle")}</EmptyTitle>
            <EmptyDescription>
              {t("title.aliasesEmptyDescription")}
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!canScan}
              onClick={() => setScanOpen(true)}
            >
              <ScanSearchIcon data-icon="inline-start" />
              {t("title.findAliasRanges")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <div className="flex flex-col gap-2">
          {visibleRanges.map((range) => (
            <RangeRow
              key={range.id}
              range={range}
              sourceVariants={sourceVariants}
              onEdit={openEdit}
              onDelete={setDeleteTarget}
            />
          ))}
        </div>
      )}

      {manualAliases.length > 0 && (
        <Collapsible open={manualOpen} onOpenChange={setManualOpen}>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex flex-col gap-1">
              <h3 className="text-sm font-medium">{t("title.manualAliases")}</h3>
              <p className="text-xs text-muted-foreground">
                {t("title.manualAliasesHint")}
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setManualOpen(!manualOpen)}
            >
              {manualOpen
                ? t("title.manualAliasesHide")
                : t("title.manualAliasesShow")}
              <ChevronDownIcon
                data-icon="inline-end"
                className={cn(
                  "transition-transform",
                  manualOpen && "rotate-180",
                )}
              />
            </Button>
          </div>
          <CollapsibleContent>
            <div className="mt-2 flex flex-col divide-y overflow-hidden rounded-xl border">
              {manualAliases.map((alias) => (
                <div
                  key={alias.alias_key}
                  className="flex items-center justify-between gap-2 p-3"
                >
                  <div className="min-w-0 text-sm">
                    <div className="truncate">
                      {aliasChapterLabel(alias.alias_key, chapters, t)}
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                      → {aliasChapterLabel(alias.canonical_key, chapters, t)}
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant={
                      confirmingAlias === alias.alias_key
                        ? "destructive"
                        : "ghost"
                    }
                    size="icon-sm"
                    aria-label={
                      confirmingAlias === alias.alias_key
                        ? t("title.confirmDeleteAlias")
                        : t("title.deleteAlias")
                    }
                    disabled={removeAlias.isPending}
                    onClick={() => deleteManual(alias.alias_key)}
                  >
                    {confirmingAlias === alias.alias_key ? (
                      <CheckIcon aria-hidden />
                    ) : (
                      <Trash2Icon aria-hidden />
                    )}
                  </Button>
                </div>
              ))}
            </div>
          </CollapsibleContent>
        </Collapsible>
      )}

      {dialogOpen && (
        <AliasRangeDialog
          titleId={titleId}
          sources={scopedSources}
          chapters={chapters}
          preferredSourceId={preferredSourceId}
          range={editing}
          open={dialogOpen}
          onOpenChange={(open) => {
            setDialogOpen(open);
            if (!open) setEditing(null);
          }}
        />
      )}

      {scanOpen && (
        <AliasScanDialog
          titleId={titleId}
          sources={scopedSources}
          chapters={chapters}
          ranges={visibleRanges}
          preferredSourceId={preferredSourceId}
          open={scanOpen}
          onOpenChange={setScanOpen}
        />
      )}

      <AlertDialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("title.rangeDeleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("title.rangeDeleteDescription", {
                count: (deleteTarget?.coverage.linked ?? 0) + (deleteTarget?.coverage.pending ?? 0),
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={remove.isPending}>
              {t("title.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={remove.isPending}
              onClick={confirmDelete}
            >
              {remove.isPending && <Spinner data-icon="inline-start" />}
              {t("title.rangeDelete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}
