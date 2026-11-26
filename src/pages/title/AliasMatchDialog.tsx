import { useMemo, useState } from "react";
import { Link2Icon } from "lucide-react";

import {
  useAddAliasesMutation,
  useErrorText,
  type ChapterAlias,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import { useT } from "@/lib/i18n";
import { SourceIcon } from "@/ui/SourceIcon";
import { Button } from "@/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui/shadcn/dialog";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/ui/shadcn/empty";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
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
  buildAliasPairs,
  collectVariants,
  formatChapterLabel,
  matchByNumber,
  suggestCanonicalSide,
  type AliasVariant,
  type CanonicalSide,
} from "./aliases";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  aliases: ChapterAlias[];
  preferredSourceId: number | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

function VariantLine({ variant }: { variant: AliasVariant }) {
  const t = useT();
  return (
    <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
      <SourceIcon code={variant.sourceCode} className="size-4" />
      <span className="shrink-0">{variant.sourceName}</span>
      <span className="truncate">{formatChapterLabel(variant, t)}</span>
      <code className="shrink-0 rounded bg-muted px-1 py-0.5">
        {variant.chapterKey}
      </code>
    </div>
  );
}

/**
 * Массовая связка глав по номеру: источники могли разойтись в томе,
 * и тогда каждая глава получает свой alias. Решение админа — чья
 * нумерация останется канонической.
 */
export function AliasMatchDialog({
  titleId,
  sources,
  chapters,
  aliases,
  preferredSourceId,
  open,
  onOpenChange,
}: Props) {
  const t = useT();
  const add = useAddAliasesMutation(titleId);
  const errorText = useErrorText(add.error);

  const sourceVariants = useMemo(
    () => collectVariants(chapters, sources, preferredSourceId),
    [chapters, sources, preferredSourceId],
  );
  const [leftChoice, setLeftChoice] = useState<number | null>(null);
  const [rightChoice, setRightChoice] = useState<number | null>(null);
  const [sideChoice, setSideChoice] = useState<CanonicalSide | null>(null);

  const leftId = leftChoice ?? sourceVariants[0]?.source.source_id ?? null;
  const rightOptions = sourceVariants.filter(
    (item) => item.source.source_id !== leftId,
  );
  const rightId =
    rightOptions.find(
      (item) => item.source.source_id === rightChoice,
    )?.source.source_id ??
    rightOptions[0]?.source.source_id ??
    null;

  const left = sourceVariants.find((item) => item.source.source_id === leftId);
  const right = sourceVariants.find((item) => item.source.source_id === rightId);

  const pairs = useMemo(
    () => matchByNumber(left?.variants ?? [], right?.variants ?? [], aliases),
    [left, right, aliases],
  );
  const side = sideChoice ?? suggestCanonicalSide(pairs);

  function handleOpenChange(next: boolean): void {
    if (!next) {
      add.reset();
      setLeftChoice(null);
      setRightChoice(null);
      setSideChoice(null);
    }
    onOpenChange(next);
  }

  function submit(): void {
    if (side === null) return;
    const payload = buildAliasPairs(pairs, side);
    if (payload.length === 0) return;
    add.mutate(payload, { onSuccess: () => handleOpenChange(false) });
  }

  const sourceItems = sourceVariants.map((item) => ({
    value: item.source.source_id,
    label: item.source.name,
  }));
  const sideSource =
    side === null ? undefined : side === "left" ? left?.source : right?.source;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Link2Icon className="size-4" aria-hidden />
            {t("title.linkByNumber")}
          </DialogTitle>
          <DialogDescription>{t("title.linkByNumberDescription")}</DialogDescription>
        </DialogHeader>

        {sourceVariants.length < 2 ? (
          <p className="text-sm text-muted-foreground">
            {t("title.linkNeedsSources")}
          </p>
        ) : (
          <div className="flex flex-col gap-4">
            <FieldGroup className="grid gap-3 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor="alias-match-left">
                  {t("title.linkSideA")}
                </FieldLabel>
                <Select
                  items={sourceItems}
                  value={leftId}
                  onValueChange={(value) => {
                    add.reset();
                    setLeftChoice(value);
                    setSideChoice(null);
                  }}
                >
                  <SelectTrigger id="alias-match-left" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {sourceItems.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>

              <Field>
                <FieldLabel htmlFor="alias-match-right">
                  {t("title.linkSideB")}
                </FieldLabel>
                <Select
                  items={rightOptions.map((item) => ({
                    value: item.source.source_id,
                    label: item.source.name,
                  }))}
                  value={rightId}
                  onValueChange={(value) => {
                    add.reset();
                    setRightChoice(value);
                    setSideChoice(null);
                  }}
                >
                  <SelectTrigger id="alias-match-right" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {rightOptions.map((item) => (
                        <SelectItem
                          key={item.source.source_id}
                          value={item.source.source_id}
                        >
                          {item.source.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
            </FieldGroup>

            <Field>
              <FieldLabel id="alias-match-numbering">
                {t("title.linkNumbering")}
              </FieldLabel>
              <ToggleGroup
                aria-labelledby="alias-match-numbering"
                value={side === null ? [] : [side]}
                size="sm"
                variant="outline"
                onValueChange={(value) => {
                  add.reset();
                  const next = value[0];
                  if (next === "left" || next === "right") setSideChoice(next);
                }}
              >
                <ToggleGroupItem value="left" disabled={left === undefined}>
                  {left?.source.name ?? "A"}
                </ToggleGroupItem>
                <ToggleGroupItem value="right" disabled={right === undefined}>
                  {right?.source.name ?? "B"}
                </ToggleGroupItem>
              </ToggleGroup>
              <FieldDescription>
                {side === null
                  ? t("title.linkNumberingRequired")
                  : t("title.linkNumberingHint", {
                      source: sideSource?.name ?? "",
                    })}
              </FieldDescription>
            </Field>

            {pairs.length === 0 ? (
              <Empty className="border">
                <EmptyHeader>
                  <EmptyTitle>{t("title.linkEmptyTitle")}</EmptyTitle>
                  <EmptyDescription>
                    {t("title.linkEmptyDescription")}
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : (
              <div className="flex flex-col gap-2">
                <p className="text-sm text-muted-foreground">
                  {t("title.linkFound", { count: pairs.length })}
                </p>
                <ScrollArea className="h-64 rounded-lg border">
                  <div className="flex flex-col divide-y px-3">
                    {pairs.map((pair) => (
                      <div
                        key={`${pair.left.chapterKey}:${pair.right.chapterKey}`}
                        className="flex flex-col gap-1 py-2"
                      >
                        <VariantLine variant={pair.left} />
                        <VariantLine variant={pair.right} />
                      </div>
                    ))}
                  </div>
                </ScrollArea>
                {errorText && (
                  <p role="alert" className="text-sm text-destructive">
                    {errorText}
                  </p>
                )}
              </div>
            )}
          </div>
        )}

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={add.isPending}
            onClick={() => handleOpenChange(false)}
          >
            {t("title.cancel")}
          </Button>
          <Button
            type="button"
            disabled={pairs.length === 0 || side === null || add.isPending}
            onClick={submit}
          >
            {add.isPending ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <Link2Icon data-icon="inline-start" />
            )}
            {t("title.linkApply", { count: pairs.length })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
