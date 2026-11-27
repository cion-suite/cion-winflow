import { useMemo, useState } from "react";
import {
  AlertTriangleIcon,
  CheckIcon,
  ChevronDownIcon,
  Link2Icon,
  Trash2Icon,
} from "lucide-react";

import {
  useCreateAliasRangeMutation,
  useDeleteAliasRangeMutation,
  useErrorText,
  useUpdateAliasRangeMutation,
  type AliasRange,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { SourceIcon } from "@/ui/SourceIcon";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/ui/shadcn/alert";
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
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
} from "@/ui/shadcn/collapsible";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@/ui/shadcn/combobox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui/shadcn/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/ui/shadcn/field";
import { ScrollArea } from "@/ui/shadcn/scroll-area";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import { Spinner } from "@/ui/shadcn/spinner";
import { ToggleGroup, ToggleGroupItem } from "@/ui/shadcn/toggle-group";
import {
  collectVariants,
  findCanonicalSide,
  formatChapterLabel,
  previewRange,
  type AliasVariant,
  type SourceVariants,
} from "./aliases";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  preferredSourceId: number | null;
  range: AliasRange | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

type SourceItem = {
  value: number;
  label: string;
};

function SourceSelect({
  id,
  value,
  items,
  onChange,
}: {
  id: string;
  value: number | null;
  items: SourceItem[];
  onChange: (value: number) => void;
}) {
  const t = useT();
  return (
    <Select
      items={items}
      value={value}
      onValueChange={(next) => {
        if (next !== null) onChange(next);
      }}
    >
      <SelectTrigger id={id} className="w-full">
        <SelectValue placeholder={t("title.rangeSelectSource")} />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          {items.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  );
}

function SourceDisplay({ source }: { source?: TitleSource }) {
  if (source === undefined) {
    return (
      <div className="flex h-9 items-center rounded-lg border px-3 text-sm text-muted-foreground">
        —
      </div>
    );
  }
  return (
    <div className="flex h-9 min-w-0 items-center gap-2 rounded-lg border px-3 text-sm">
      <SourceIcon code={source.code} className="size-4" />
      <span className="truncate">{source.name}</span>
    </div>
  );
}

function ChapterCombobox({
  id,
  variants,
  value,
  onChange,
  disabled,
}: {
  id: string;
  variants: readonly AliasVariant[];
  value: string | null;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const t = useT();
  const labels = useMemo(() => {
    const map = new Map<string, string>();
    for (const variant of variants) {
      map.set(variant.chapterKey, formatChapterLabel(variant, t));
    }
    return map;
  }, [variants, t]);
  const items = useMemo(
    () => variants.map((variant) => variant.chapterKey),
    [variants],
  );

  return (
    <Combobox
      items={items}
      value={value}
      onValueChange={(next) => {
        if (typeof next === "string") onChange(next);
      }}
      itemToStringLabel={(key) => labels.get(key) ?? key}
    >
      <ComboboxInput
        id={id}
        placeholder={t("title.rangeSelectChapter")}
        showClear
        disabled={disabled}
      />
      <ComboboxContent>
        <ComboboxEmpty>{t("title.chaptersNoMatch")}</ComboboxEmpty>
        <ComboboxList>
          {(key: string) => (
            <ComboboxItem key={key} value={key}>
              {labels.get(key) ?? key}
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  );
}

function sideVariant(
  side: SourceVariants | undefined,
  key: string | null,
): AliasVariant | undefined {
  return side?.variants.find((variant) => variant.chapterKey === key);
}

export function AliasRangeDialog({
  titleId,
  sources,
  chapters,
  preferredSourceId,
  range,
  open,
  onOpenChange,
}: Props) {
  const t = useT();
  const sourceVariants = useMemo(
    () => collectVariants(chapters, sources, preferredSourceId),
    [chapters, sources, preferredSourceId],
  );

  const initialLinked = range
    ? sourceVariants.find(
        (item) => item.source.source_entry_id === range.source_entry_id,
      )
    : sourceVariants[0];
  const initialCanonical = range
    ? (findCanonicalSide(
        sourceVariants,
        range.canonical.chapter_key,
        range.canonical.chapter_id,
      ) ??
      sourceVariants.find(
        (item) =>
          item.source.source_id !== initialLinked?.source.source_id,
      ))
    : sourceVariants.find(
        (item) =>
          item.source.source_id !== initialLinked?.source.source_id,
      );

  const [linkedSourceId, setLinkedSourceId] = useState<number | null>(
    initialLinked?.source.source_id ?? null,
  );
  const [canonicalSourceId, setCanonicalSourceId] = useState<number | null>(
    initialCanonical?.source.source_id ?? null,
  );
  const [linkedChapterKey, setLinkedChapterKey] = useState<string | null>(
    range !== null &&
      initialLinked?.variants.some(
        (variant) => variant.chapterKey === range.anchor.chapter_key,
      )
      ? range.anchor.chapter_key
      : null,
  );
  const [canonicalChapterKey, setCanonicalChapterKey] = useState<string | null>(
    range !== null &&
      initialCanonical?.variants.some(
        (variant) => variant.chapterKey === range.canonical.chapter_key,
      )
      ? range.canonical.chapter_key
      : null,
  );
  const [showPairs, setShowPairs] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const create = useCreateAliasRangeMutation(titleId);
  const update = useUpdateAliasRangeMutation(titleId);
  const remove = useDeleteAliasRangeMutation(titleId);
  const errorText = useErrorText(
    create.error ?? update.error ?? remove.error,
  );

  const linkedSource = sourceVariants.find(
    (item) => item.source.source_id === linkedSourceId,
  );
  const canonicalSource = sourceVariants.find(
    (item) => item.source.source_id === canonicalSourceId,
  );
  const linkedVariant = sideVariant(linkedSource, linkedChapterKey);
  const canonicalVariant = sideVariant(canonicalSource, canonicalChapterKey);

  const sourceItems = useMemo<SourceItem[]>(
    () =>
      sourceVariants.map((item) => ({
        value: item.source.source_id,
        label: item.source.name,
      })),
    [sourceVariants],
  );
  const canonicalSourceItems = sourceItems.filter(
    (item) => item.value !== linkedSourceId,
  );

  const validationMessage = useMemo(() => {
    if (linkedSource === undefined || canonicalSource === undefined) {
      return t("title.rangeNoSources");
    }
    if (linkedSource.source.source_id === canonicalSource.source.source_id) {
      return t("title.rangeInvalidSameSource");
    }
    if (linkedVariant === undefined || canonicalVariant === undefined) {
      return null;
    }
    if (linkedVariant.chapterKind !== canonicalVariant.chapterKind) {
      return t("title.rangeInvalidKind");
    }
    if (
      linkedVariant.numberMilli === null ||
      canonicalVariant.numberMilli === null ||
      linkedVariant.numberText === null ||
      canonicalVariant.numberText === null
    ) {
      return t("title.rangeInvalidNumber");
    }
    if (linkedVariant.chapterKey === canonicalVariant.chapterKey) {
      return t("title.rangeSameKey");
    }
    if (
      (linkedVariant.volume === null) !==
      (canonicalVariant.volume === null)
    ) {
      return t("title.rangeInvalidVolume");
    }
    return null;
  }, [canonicalSource, canonicalVariant, linkedSource, linkedVariant, t]);

  const preview = useMemo(
    () =>
      previewRange(
        linkedSource?.variants ?? [],
        canonicalSource?.variants ?? [],
        linkedChapterKey,
        canonicalChapterKey,
      ),
    [
      canonicalChapterKey,
      canonicalSource,
      linkedChapterKey,
      linkedSource,
    ],
  );

  const dirty =
    range === null ||
    linkedSource?.source.source_entry_id !== range.source_entry_id ||
    linkedVariant?.chapterKey !== range.anchor.chapter_key ||
    canonicalVariant?.chapterKey !== range.canonical.chapter_key;

  const pending = create.isPending || update.isPending;
  const canSubmit =
    validationMessage === null &&
    linkedVariant !== undefined &&
    canonicalVariant !== undefined &&
    dirty &&
    !pending;

  function chooseCanonical(nextSourceId: number): void {
    if (nextSourceId === canonicalSourceId || linkedSourceId === null) return;
    setLinkedSourceId(canonicalSourceId);
    setCanonicalSourceId(linkedSourceId);
    setLinkedChapterKey(canonicalChapterKey);
    setCanonicalChapterKey(linkedChapterKey);
  }

  function changeLinkedSource(nextSourceId: number): void {
    setLinkedSourceId(nextSourceId);
    setLinkedChapterKey(null);
    if (nextSourceId === canonicalSourceId) {
      const fallback = sourceVariants.find(
        (item) => item.source.source_id !== nextSourceId,
      );
      setCanonicalSourceId(fallback?.source.source_id ?? null);
      setCanonicalChapterKey(null);
    }
  }

  function selectLinkedChapter(key: string): void {
    setLinkedChapterKey(key);
    // Автоподбор только пока вторая сторона не выбрана: иначе нельзя
    // связать главы с разными номерами (второй алиас после смены тома).
    if (canonicalChapterKey !== null) return;
    const variant = sideVariant(linkedSource, key);
    if (variant === undefined || canonicalSource === undefined) return;
    const matches = canonicalSource.variants.filter(
      (item) =>
        item.chapterKind === variant.chapterKind &&
        item.numberText === variant.numberText,
    );
    if (matches.length === 1) {
      setCanonicalChapterKey(matches[0].chapterKey);
    }
  }

  function selectCanonicalChapter(key: string): void {
    setCanonicalChapterKey(key);
    if (linkedChapterKey !== null) return;
    const variant = sideVariant(canonicalSource, key);
    if (variant === undefined || linkedSource === undefined) return;
    const matches = linkedSource.variants.filter(
      (item) =>
        item.chapterKind === variant.chapterKind &&
        item.numberText === variant.numberText,
    );
    if (matches.length === 1) {
      setLinkedChapterKey(matches[0].chapterKey);
    }
  }

  function submit(): void {
    if (linkedSource === undefined || canonicalSource === undefined) return;
    if (linkedVariant === undefined || canonicalVariant === undefined) return;
    const input = {
      source_entry_id: linkedSource.source.source_entry_id,
      anchor_chapter_id: linkedVariant.chapterId,
      canonical_chapter_id: canonicalVariant.chapterId,
    };
    if (range === null) {
      create.mutate(input, { onSuccess: () => onOpenChange(false) });
    } else {
      update.mutate(
        { rangeId: range.id, input },
        { onSuccess: () => onOpenChange(false) },
      );
    }
  }

  function confirmDelete(): void {
    if (range === null) return;
    remove.mutate(range.id, {
      onSuccess: () => {
        setDeleteOpen(false);
        onOpenChange(false);
      },
    });
  }

  const status =
    range?.status === "needs_review"
      ? t("title.rangeStatusNeedsReview")
      : range?.status === "broken"
        ? t("title.rangeStatusBroken")
        : null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <div className="flex flex-wrap items-center gap-2">
            <DialogTitle>
              {range === null
                ? t("title.newAliasRangeTitle")
                : t("title.aliasRangeTitle")}
            </DialogTitle>
            {status !== null && <Badge variant="outline">{status}</Badge>}
            {range !== null && range.mapping_mode === "number_only" && (
              <Badge variant="outline">
                {t("title.rangeWarningNumberOnly")}
              </Badge>
            )}
          </div>
          <DialogDescription>
            {range === null
              ? t("title.aliasRangesHint")
              : t("title.rangeEditHint")}
          </DialogDescription>
        </DialogHeader>

        {sourceVariants.length < 2 ? (
          <Alert>
            <AlertTriangleIcon aria-hidden />
            <AlertTitle>{t("title.aliasRangeTitle")}</AlertTitle>
            <AlertDescription>{t("title.rangeNoSources")}</AlertDescription>
          </Alert>
        ) : (
          <div className="flex flex-col gap-4">
            <div className="grid gap-3 sm:grid-cols-[1fr_auto_1fr] sm:items-start">
              <FieldSet className="min-w-0 gap-3 rounded-xl border p-3">
                <FieldLegend variant="label">
                  {t("title.linkedChapter")}
                </FieldLegend>
                <FieldGroup className="gap-2">
                  <Field>
                    <FieldLabel htmlFor="range-linked-source">
                      {t("title.rangeSelectSource")}
                    </FieldLabel>
                    {range === null ? (
                      <SourceSelect
                        id="range-linked-source"
                        value={linkedSourceId}
                        items={sourceItems}
                        onChange={changeLinkedSource}
                      />
                    ) : (
                      <SourceDisplay source={linkedSource?.source} />
                    )}
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="range-linked-chapter">
                      {t("title.rangeSelectChapter")}
                    </FieldLabel>
                    <ChapterCombobox
                      id="range-linked-chapter"
                      variants={linkedSource?.variants ?? []}
                      value={linkedChapterKey}
                      onChange={selectLinkedChapter}
                    />
                  </Field>
                </FieldGroup>
              </FieldSet>

              <div className="hidden items-start justify-center pt-10 sm:flex">
                <span className="grid size-8 place-items-center rounded-full border text-muted-foreground">
                  <Link2Icon className="size-4" />
                </span>
              </div>

              <FieldSet className="min-w-0 gap-3 rounded-xl border p-3">
                <FieldLegend variant="label">
                  {t("title.canonicalChapter")}
                </FieldLegend>
                <FieldGroup className="gap-2">
                  <Field>
                    <FieldLabel htmlFor="range-canonical-source">
                      {t("title.rangeSelectSource")}
                    </FieldLabel>
                    {range === null ? (
                      <SourceSelect
                        id="range-canonical-source"
                        value={canonicalSourceId}
                        items={canonicalSourceItems}
                        onChange={(value) => {
                          setCanonicalSourceId(value);
                          setCanonicalChapterKey(null);
                        }}
                      />
                    ) : (
                      <SourceDisplay source={canonicalSource?.source} />
                    )}
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="range-canonical-chapter">
                      {t("title.rangeSelectChapter")}
                    </FieldLabel>
                    <ChapterCombobox
                      id="range-canonical-chapter"
                      variants={canonicalSource?.variants ?? []}
                      value={canonicalChapterKey}
                      onChange={selectCanonicalChapter}
                    />
                  </Field>
                </FieldGroup>
              </FieldSet>
            </div>

            <Field>
              <FieldLabel id="range-canonical-numbering">
                {t("title.canonicalNumbering")}
              </FieldLabel>
              <ToggleGroup
                aria-labelledby="range-canonical-numbering"
                value={canonicalSource === undefined ? [] : [String(canonicalSource.source.source_id)]}
                onValueChange={(value) => {
                  const next = Number(value[0]);
                  if (Number.isFinite(next)) chooseCanonical(next);
                }}
                variant="outline"
                size="sm"
                className="w-full"
              >
                {[linkedSource, canonicalSource].map((side) =>
                  side === undefined ? null : (
                    <ToggleGroupItem
                      key={side.source.source_id}
                      value={String(side.source.source_id)}
                      className="min-w-0 flex-1"
                    >
                      <SourceIcon
                        code={side.source.code}
                        className="size-4"
                      />
                      <span className="truncate">{side.source.name}</span>
                      {canonicalSource?.source.source_id ===
                        side.source.source_id && (
                        <CheckIcon data-icon="inline-end" />
                      )}
                    </ToggleGroupItem>
                  ),
                )}
              </ToggleGroup>
              <FieldDescription>
                {t("title.canonicalNumberingHint")}
              </FieldDescription>
            </Field>

            <Collapsible open={showPairs} onOpenChange={setShowPairs}>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-sm text-muted-foreground">
                  {validationMessage ?? t("title.rangePreview", {
                    linked: preview.linked,
                    pending: preview.pending,
                  })}
                </p>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setShowPairs(!showPairs)}
                >
                  {showPairs
                    ? t("title.rangeHidePairs")
                    : t("title.rangeShowPairs")}
                  <ChevronDownIcon
                    data-icon="inline-end"
                    className={cn(
                      "transition-transform",
                      showPairs && "rotate-180",
                    )}
                  />
                </Button>
              </div>
              <CollapsibleContent>
                <ScrollArea className="mt-2 h-40 rounded-lg border">
                  <div className="flex flex-col divide-y px-3">
                    {preview.pairs.slice(0, 20).map((pair) => (
                      <div
                        key={`${pair.alias.chapterKey}:${pair.canonical.chapterKey}`}
                        className="flex flex-col gap-1 py-2 text-xs"
                      >
                        <span className="truncate">
                          {pair.alias.sourceName} ·{" "}
                          {formatChapterLabel(pair.alias, t)}
                        </span>
                        <span className="truncate text-muted-foreground">
                          → {pair.canonical.sourceName} ·{" "}
                          {formatChapterLabel(pair.canonical, t)}
                        </span>
                      </div>
                    ))}
                    {preview.pairs.length === 0 && (
                      <p className="py-2 text-xs text-muted-foreground">
                        {t("title.rangePairsEmpty")}
                      </p>
                    )}
                  </div>
                </ScrollArea>
              </CollapsibleContent>
            </Collapsible>

            {validationMessage !== null &&
              linkedVariant !== undefined &&
              canonicalVariant !== undefined && (
                <Alert variant="destructive">
                  <AlertTriangleIcon aria-hidden />
                  <AlertTitle>{t("title.aliasRangeTitle")}</AlertTitle>
                  <AlertDescription>{validationMessage}</AlertDescription>
                </Alert>
              )}

            {errorText && (
              <Alert variant="destructive">
                <AlertTriangleIcon aria-hidden />
                <AlertTitle>{t("errors.unknown")}</AlertTitle>
                <AlertDescription>{errorText}</AlertDescription>
              </Alert>
            )}
          </div>
        )}

        <DialogFooter className="sm:justify-between">
          <div>
            {range !== null && (
              <Button
                type="button"
                variant="ghost"
                className="text-destructive"
                disabled={remove.isPending}
                onClick={() => setDeleteOpen(true)}
              >
                <Trash2Icon data-icon="inline-start" />
                {t("title.rangeDelete")}
              </Button>
            )}
          </div>
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={pending}
              onClick={() => onOpenChange(false)}
            >
              {t("title.cancel")}
            </Button>
            <Button type="button" disabled={!canSubmit} onClick={submit}>
              {pending && <Spinner data-icon="inline-start" />}
              {range === null
                ? t("title.rangeCreate")
                : t("title.rangeSave")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("title.rangeDeleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("title.rangeDeleteDescription", {
                count:
                  (range?.coverage.linked ?? 0) +
                  (range?.coverage.pending ?? 0),
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
    </Dialog>
  );
}
