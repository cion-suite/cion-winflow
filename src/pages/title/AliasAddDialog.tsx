import { useState } from "react";
import { PlusIcon } from "lucide-react";

import {
  useAddAliasesMutation,
  useErrorText,
  type ChapterAlias,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
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
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/ui/shadcn/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import { Spinner } from "@/ui/shadcn/spinner";
import {
  collectVariants,
  formatChapterLabel,
  type SourceVariants,
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

type ChapterFieldProps = {
  id: string;
  label: string;
  sourceVariants: SourceVariants[];
  sourceId: number | null;
  chapterKey: string | null;
  /** Ключи, которые уже участвуют в alias: повторная связка сделала бы цепочку. */
  blockedKeys: ReadonlySet<string>;
  onSourceChange: (sourceId: number) => void;
  onChapterChange: (chapterKey: string) => void;
};

function ChapterField({
  id,
  label,
  sourceVariants,
  sourceId,
  chapterKey,
  blockedKeys,
  onSourceChange,
  onChapterChange,
}: ChapterFieldProps) {
  const t = useT();
  const selected = sourceVariants.find(
    (item) => item.source.source_id === sourceId,
  );
  const sourceItems = sourceVariants.map((item) => ({
    value: item.source.source_id,
    label: item.source.name,
  }));
  // Один ключ может встретиться у нескольких групп перевода: для alias
  // это одна и та же глава, поэтому в списке оставляем одну строку.
  const chapterOptions = new Map<string, string>();
  for (const variant of selected?.variants ?? []) {
    if (blockedKeys.has(variant.chapterKey)) continue;
    if (!chapterOptions.has(variant.chapterKey)) {
      chapterOptions.set(
        variant.chapterKey,
        `${formatChapterLabel(variant, t)} · ${variant.chapterKey}`,
      );
    }
  }
  const chapterItems = [
    { value: null as string | null, label: t("title.chapterSelect") },
    ...[...chapterOptions].map(([value, label]) => ({
      value: value as string | null,
      label,
    })),
  ];

  return (
    <FieldSet className="min-w-0 gap-3 rounded-xl border p-3">
      <FieldLegend variant="label">{label}</FieldLegend>
      <FieldGroup className="gap-2">
        <Field>
          <FieldLabel htmlFor={`${id}-source`}>{t("title.source")}</FieldLabel>
          <Select
            items={sourceItems}
            value={sourceId}
            onValueChange={(value) => {
              if (value !== null) onSourceChange(value);
            }}
          >
            <SelectTrigger id={`${id}-source`} className="w-full">
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
          <FieldLabel htmlFor={`${id}-chapter`}>
            {t("title.chapterSelect")}
          </FieldLabel>
          <Select
            items={chapterItems}
            value={chapterKey}
            onValueChange={(value) => {
              if (value !== null) onChapterChange(value);
            }}
          >
            <SelectTrigger id={`${id}-chapter`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {chapterItems.map((item) => (
                  <SelectItem key={item.value ?? "placeholder"} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </FieldGroup>
    </FieldSet>
  );
}

/**
 * Ручная связка: две версии одной главы выбираются по источнику и номеру,
 * без ввода сырых ключей. Каноническая сторона задаёт том и номер в списке.
 */
export function AliasAddDialog({
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

  const sourceVariants = collectVariants(chapters, sources, preferredSourceId);
  // Перепривязка существующего alias оставила бы прогресс на старом
  // каноническом ключе, поэтому уже связанные ключи скрываем: сначала
  // удалите alias, затем свяжите заново.
  const aliasKeys = new Set(aliases.map((alias) => alias.alias_key));
  const linkedKeys = new Set([
    ...aliasKeys,
    ...aliases.map((alias) => alias.canonical_key),
  ]);
  const [aliasSourceChoice, setAliasSourceChoice] = useState<number | null>(null);
  const [canonicalSourceChoice, setCanonicalSourceChoice] = useState<number | null>(
    null,
  );
  const [aliasKey, setAliasKey] = useState<string | null>(null);
  const [canonicalKey, setCanonicalKey] = useState<string | null>(null);

  const aliasSourceId =
    aliasSourceChoice ?? sourceVariants[0]?.source.source_id ?? null;
  const canonicalSourceId =
    canonicalSourceChoice ??
    sourceVariants[1]?.source.source_id ??
    sourceVariants[0]?.source.source_id ??
    null;

  const canSubmit =
    aliasKey !== null && canonicalKey !== null && aliasKey !== canonicalKey;

  function handleOpenChange(next: boolean): void {
    if (!next) {
      add.reset();
      setAliasSourceChoice(null);
      setCanonicalSourceChoice(null);
      setAliasKey(null);
      setCanonicalKey(null);
    }
    onOpenChange(next);
  }

  function submit(): void {
    if (aliasKey === null || canonicalKey === null) return;
    if (aliasKey === canonicalKey) return;
    add.mutate(
      [{ alias_key: aliasKey, canonical_key: canonicalKey }],
      { onSuccess: () => handleOpenChange(false) },
    );
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <PlusIcon className="size-4" aria-hidden />
            {t("title.addAliasTitle")}
          </DialogTitle>
          <DialogDescription>{t("title.addAliasDescription")}</DialogDescription>
        </DialogHeader>

        {sourceVariants.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("title.linkNeedsSources")}
          </p>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <ChapterField
                id="alias-manual"
                label={t("title.aliasChapter")}
                sourceVariants={sourceVariants}
                sourceId={aliasSourceId}
                chapterKey={aliasKey}
                blockedKeys={linkedKeys}
                onSourceChange={(value) => {
                  setAliasSourceChoice(value);
                  setAliasKey(null);
                  add.reset();
                }}
                onChapterChange={(value) => {
                  setAliasKey(value);
                  add.reset();
                }}
              />
              <ChapterField
                id="alias-canonical"
                label={t("title.canonicalChapter")}
                sourceVariants={sourceVariants}
                sourceId={canonicalSourceId}
                chapterKey={canonicalKey}
                blockedKeys={aliasKeys}
                onSourceChange={(value) => {
                  setCanonicalSourceChoice(value);
                  setCanonicalKey(null);
                  add.reset();
                }}
                onChapterChange={(value) => {
                  setCanonicalKey(value);
                  add.reset();
                }}
              />
            </div>

            {aliasKey !== null && aliasKey === canonicalKey && (
              <p className="text-sm text-muted-foreground">
                {t("title.aliasSameKey")}
              </p>
            )}
            {errorText && (
              <p role="alert" className="text-sm text-destructive">
                {errorText}
              </p>
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
            disabled={!canSubmit || add.isPending}
            onClick={submit}
          >
            {add.isPending ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <PlusIcon data-icon="inline-start" />
            )}
            {t("title.addAlias")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
