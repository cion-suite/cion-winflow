import { useMemo } from "react";

import type { ChapterAlias, TitleChapter, TitleSource } from "@/api";
import { useT } from "@/lib/i18n";
import { SourceIcon } from "@/ui/SourceIcon";
import { Badge } from "@/ui/shadcn/badge";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/ui/shadcn/item";
import { AliasesPanel } from "./AliasesPanel";
import { sourceOrder } from "./aliases";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  aliases: ChapterAlias[];
  preferredSourceId: number | null;
};

/**
 * Порядок, в котором сервер ищет главу: первый источник, где она доступна,
 * и отдаёт контент. Это ответ на «какой источник работает после какого».
 */
function SourceOrder({
  sources,
  preferredSourceId,
}: {
  sources: TitleSource[];
  preferredSourceId: number | null;
}) {
  const t = useT();
  const ordered = useMemo(
    () => sourceOrder(sources, preferredSourceId),
    [sources, preferredSourceId],
  );

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <h2 className="text-sm font-medium">{t("title.sourceOrder")}</h2>
        <p className="max-w-prose text-sm text-muted-foreground">
          {t("title.sourceOrderHint")}
        </p>
      </div>

      {ordered.length === 0 ? (
        <p className="rounded-xl border border-dashed p-6 text-sm text-muted-foreground">
          {t("title.sourcesEmpty")}
        </p>
      ) : (
        <ItemGroup className="gap-0 divide-y overflow-hidden rounded-xl border">
          {ordered.map((source, index) => (
            <Item
              key={source.source_entry_id}
              size="sm"
              className="rounded-none"
            >
              <span className="grid size-7 shrink-0 place-items-center rounded-md border text-xs text-muted-foreground tabular-nums">
                {index + 1}
              </span>
              <ItemMedia className="size-8 rounded-md bg-muted p-1">
                <SourceIcon code={source.code} />
              </ItemMedia>
              <ItemContent>
                <ItemTitle>{source.name}</ItemTitle>
                <ItemDescription>
                  {t("title.sourceStats", {
                    chapters: source.chapter_count,
                    available: source.available_count,
                  })}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                {source.source_id === preferredSourceId && (
                  <Badge>{t("title.preferredSource")}</Badge>
                )}
                {!source.enabled && (
                  <Badge variant="outline">{t("title.sourceDisabled")}</Badge>
                )}
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      )}
    </section>
  );
}

export function TitleSettingsTab({
  titleId,
  sources,
  chapters,
  aliases,
  preferredSourceId,
}: Props) {
  return (
    <div className="flex flex-col gap-6">
      <SourceOrder sources={sources} preferredSourceId={preferredSourceId} />
      <AliasesPanel
        titleId={titleId}
        sources={sources}
        chapters={chapters}
        aliases={aliases}
        preferredSourceId={preferredSourceId}
      />
    </div>
  );
}
