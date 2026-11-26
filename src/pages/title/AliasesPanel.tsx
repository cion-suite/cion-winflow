import { useMemo, useState } from "react";
import {
  CheckIcon,
  Link2Icon,
  PlusIcon,
  Trash2Icon,
} from "lucide-react";

import {
  useDeleteAliasMutation,
  useErrorText,
  type ChapterAlias,
  type ChapterVariant,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import { useT } from "@/lib/i18n";
import { SourceIcon } from "@/ui/SourceIcon";
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
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/ui/shadcn/item";
import {
  formatChapterLabel,
  groupAliases,
  type AliasGroup,
} from "./aliases";
import { AliasAddDialog } from "./AliasAddDialog";
import { AliasMatchDialog } from "./AliasMatchDialog";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  aliases: ChapterAlias[];
  preferredSourceId: number | null;
};

function VariantRow({
  variant,
  canonicalKey,
  selected,
  isAlias,
  confirming,
  pending,
  onDelete,
}: {
  variant: ChapterVariant;
  canonicalKey: string;
  selected: boolean;
  isAlias: boolean;
  confirming: boolean;
  pending: boolean;
  onDelete: () => void;
}) {
  const t = useT();
  const unavailable =
    !variant.source_enabled ||
    variant.availability === "locked" ||
    variant.availability === "missing";
  const isCanonical = variant.chapter_key === canonicalKey;

  return (
    <Item size="sm" className="rounded-none border-transparent">
      <ItemMedia className="size-5">
        <SourceIcon code={variant.source_code} />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>{variant.source_name}</ItemTitle>
        <ItemDescription className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <span className="truncate">
            {formatChapterLabel(
              { volume: variant.volume, numberText: variant.number_text },
              t,
            )}
          </span>
          <code className="truncate rounded bg-muted px-1.5 py-0.5 text-xs">
            {variant.chapter_key}
          </code>
        </ItemDescription>
      </ItemContent>

      <ItemActions className="gap-1">
        {isCanonical && (
          <Badge variant="outline">{t("title.aliasCanonical")}</Badge>
        )}
        {selected && (
          <Badge variant="secondary">{t("title.aliasSelected")}</Badge>
        )}
        {unavailable && (
          <Badge variant="outline">{t("title.aliasUnavailable")}</Badge>
        )}
        {isAlias && (
          <Button
            type="button"
            variant={confirming ? "destructive" : "ghost"}
            size="icon-sm"
            aria-label={
              confirming ? t("title.confirmDeleteAlias") : t("title.deleteAlias")
            }
            disabled={pending}
            onClick={onDelete}
          >
            {confirming ? <CheckIcon aria-hidden /> : <Trash2Icon aria-hidden />}
          </Button>
        )}
      </ItemActions>
    </Item>
  );
}

function GroupRow({
  group,
  confirming,
  pending,
  onDelete,
}: {
  group: AliasGroup;
  confirming: string | null;
  pending: boolean;
  onDelete: (aliasKey: string) => void;
}) {
  const t = useT();
  const variants = group.chapter?.variants ?? [];
  const variantKeys = new Set(variants.map((variant) => variant.chapter_key));
  const orphanKeys = [...group.aliasKeys].filter((key) => !variantKeys.has(key));

  return (
    <div className="rounded-xl border">
      <div className="flex items-center justify-between gap-3 border-b px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <Link2Icon aria-hidden className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate text-sm font-medium">
            {group.chapter === null
              ? group.canonicalKey
              : formatChapterLabel(
                  {
                    volume: group.chapter.volume,
                    numberText: group.chapter.number_text,
                  },
                  t,
                )}
          </span>
          <code className="hidden shrink-0 text-xs text-muted-foreground sm:inline">
            {group.canonicalKey}
          </code>
        </div>
        <Badge variant="secondary">{group.aliasKeys.size}</Badge>
      </div>

      <ItemGroup className="gap-0 divide-y">
        {variants.map((variant) => {
          const isAlias = group.aliasKeys.has(variant.chapter_key);
          return (
            <VariantRow
              key={variant.chapter_id}
              variant={variant}
              canonicalKey={group.canonicalKey}
              selected={group.chapter?.selected_chapter_id === variant.chapter_id}
              isAlias={isAlias}
              confirming={confirming === variant.chapter_key}
              pending={pending}
              onDelete={() => onDelete(variant.chapter_key)}
            />
          );
        })}

        {orphanKeys.map((key) => (
          <Item
            key={key}
            size="sm"
            className="rounded-none border-transparent"
          >
            <ItemContent>
              <ItemDescription className="flex items-center gap-2">
                <code className="truncate rounded bg-muted px-1.5 py-0.5 text-xs">
                  {key}
                </code>
                <span className="text-xs">{t("title.aliasOrphan")}</span>
              </ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button
                type="button"
                variant={confirming === key ? "destructive" : "ghost"}
                size="icon-sm"
                aria-label={
                  confirming === key
                    ? t("title.confirmDeleteAlias")
                    : t("title.deleteAlias")
                }
                disabled={pending}
                onClick={() => onDelete(key)}
              >
                {confirming === key ? (
                  <CheckIcon aria-hidden />
                ) : (
                  <Trash2Icon aria-hidden />
                )}
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
    </div>
  );
}

export function AliasesPanel({
  titleId,
  sources,
  chapters,
  aliases,
  preferredSourceId,
}: Props) {
  const t = useT();
  const remove = useDeleteAliasMutation(titleId);
  const removeError = useErrorText(remove.error);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [matchOpen, setMatchOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);

  const groups = useMemo(
    () => groupAliases(aliases, chapters),
    [aliases, chapters],
  );
  const sourceCount = useMemo(() => {
    const ids = new Set<number>();
    for (const chapter of chapters) {
      for (const variant of chapter.variants) ids.add(variant.source_id);
    }
    return ids.size;
  }, [chapters]);

  function deleteAlias(aliasKey: string): void {
    if (confirming === aliasKey) {
      remove.mutate(aliasKey, { onSuccess: () => setConfirming(null) });
      return;
    }
    setConfirming(aliasKey);
  }

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 className="text-sm font-medium">{t("title.aliases")}</h2>
          <p className="max-w-prose text-sm text-muted-foreground">
            {t("title.aliasesHint")}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={sourceCount < 2}
            onClick={() => setMatchOpen(true)}
          >
            <Link2Icon data-icon="inline-start" />
            {t("title.linkByNumber")}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={sourceCount === 0}
            onClick={() => setAddOpen(true)}
          >
            <PlusIcon data-icon="inline-start" />
            {t("title.addAlias")}
          </Button>
        </div>
      </div>

      {removeError && (
        <p role="alert" className="text-sm text-destructive">
          {removeError}
        </p>
      )}

      {groups.length === 0 ? (
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
              disabled={sourceCount < 2}
              onClick={() => setMatchOpen(true)}
            >
              <Link2Icon data-icon="inline-start" />
              {t("title.linkByNumber")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <div className="flex flex-col gap-2">
          {groups.map((group) => (
            <GroupRow
              key={group.canonicalKey}
              group={group}
              confirming={confirming}
              pending={remove.isPending}
              onDelete={deleteAlias}
            />
          ))}
        </div>
      )}

      <AliasMatchDialog
        titleId={titleId}
        sources={sources}
        chapters={chapters}
        aliases={aliases}
        preferredSourceId={preferredSourceId}
        open={matchOpen}
        onOpenChange={setMatchOpen}
      />
      <AliasAddDialog
        titleId={titleId}
        sources={sources}
        chapters={chapters}
        aliases={aliases}
        preferredSourceId={preferredSourceId}
        open={addOpen}
        onOpenChange={setAddOpen}
      />
    </section>
  );
}
