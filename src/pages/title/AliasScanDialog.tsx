import { useMemo, useState } from "react";
import {
  AlertTriangleIcon,
  CheckIcon,
  ScanSearchIcon,
} from "lucide-react";

import {
  useApplyAliasRangesMutation,
  useErrorText,
  type AliasRange,
  type AliasRangeInput,
  type TitleChapter,
  type TitleSource,
} from "@/api";
import { useT } from "@/lib/i18n";
import { SourceIcon } from "@/ui/SourceIcon";
import { Alert, AlertDescription, AlertTitle } from "@/ui/shadcn/alert";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import { Checkbox } from "@/ui/shadcn/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
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
import { ScrollArea } from "@/ui/shadcn/scroll-area";
import { Spinner } from "@/ui/shadcn/spinner";
import { ToggleGroup, ToggleGroupItem } from "@/ui/shadcn/toggle-group";
import {
  formatChapterLabel,
  proposeRanges,
  rangeInputForProposal,
  type RangeProposal,
} from "./aliases";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  ranges: AliasRange[];
  preferredSourceId: number | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

function ProposalCard({
  proposal,
  selected,
  side,
  onSelectedChange,
  onSideChange,
}: {
  proposal: RangeProposal;
  selected: boolean;
  side: "a" | "b";
  onSelectedChange: (selected: boolean) => void;
  onSideChange: (side: "a" | "b") => void;
}) {
  const t = useT();
  const sideA = proposal.a.source;
  const sideB = proposal.b.source;

  return (
    <div className="flex flex-col gap-2 rounded-xl border p-3">
      <div className="flex items-start gap-3">
        <Checkbox
          checked={selected}
          onCheckedChange={(checked) => onSelectedChange(checked === true)}
          aria-label={t("title.scanProposals")}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium">
              {sideA.name} ↔ {sideB.name}
            </span>
            <Badge variant="secondary">
              {t("title.scanProposalCoverage", { count: proposal.count })}
            </Badge>
          </div>
          <div className="flex flex-col gap-0.5 text-xs text-muted-foreground">
            <span className="truncate">
              {sideA.name} · {formatChapterLabel(proposal.anchorA, t)}
            </span>
            <span className="truncate">
              {sideB.name} · {formatChapterLabel(proposal.anchorB, t)}
            </span>
          </div>
        </div>
      </div>

      <div className="flex flex-col gap-1.5 pl-7">
        <span className="text-xs text-muted-foreground">
          {t("title.scanCanonicalSide")}
        </span>
        <ToggleGroup
          value={[side]}
          onValueChange={(value) => {
            const next = value[0];
            if (next === "a" || next === "b") onSideChange(next);
          }}
          variant="outline"
          size="sm"
          disabled={!selected}
        >
          <ToggleGroupItem value="a">
            <SourceIcon code={sideA.code} className="size-4" />
            <span className="truncate">{sideA.name}</span>
            {side === "a" && <CheckIcon data-icon="inline-end" />}
          </ToggleGroupItem>
          <ToggleGroupItem value="b">
            <SourceIcon code={sideB.code} className="size-4" />
            <span className="truncate">{sideB.name}</span>
            {side === "b" && <CheckIcon data-icon="inline-end" />}
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
    </div>
  );
}

export function AliasScanDialog({
  titleId,
  sources,
  chapters,
  ranges,
  preferredSourceId,
  open,
  onOpenChange,
}: Props) {
  const t = useT();
  const { proposals, notes } = useMemo(
    () => proposeRanges(chapters, sources, ranges, preferredSourceId),
    [chapters, preferredSourceId, ranges, sources],
  );
  const [selected, setSelected] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(proposals.map((proposal) => [proposal.id, true])),
  );
  const [sides, setSides] = useState<Record<string, "a" | "b">>(() =>
    Object.fromEntries(
      proposals.map((proposal) => [proposal.id, proposal.suggestedCanonical]),
    ),
  );
  const apply = useApplyAliasRangesMutation(titleId);
  const errorText = useErrorText(apply.error);

  const inputs: AliasRangeInput[] = proposals
    .filter((proposal) => selected[proposal.id] === true)
    .map((proposal) =>
      rangeInputForProposal(
        proposal,
        sides[proposal.id] ?? proposal.suggestedCanonical,
      ),
    );

  function submit(): void {
    if (inputs.length === 0) return;
    apply.mutate(inputs, { onSuccess: () => onOpenChange(false) });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ScanSearchIcon className="size-4" aria-hidden />
            {t("title.scanTitle")}
          </DialogTitle>
          <DialogDescription>{t("title.scanDescription")}</DialogDescription>
        </DialogHeader>

        {proposals.length === 0 && notes.length === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <ScanSearchIcon />
              </EmptyMedia>
              <EmptyTitle>{t("title.scanEmpty")}</EmptyTitle>
              <EmptyDescription>{t("title.aliasRangesHint")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="flex flex-col gap-4">
            {proposals.length > 0 && (
              <div className="flex flex-col gap-2">
                <span className="text-sm font-medium">
                  {t("title.scanProposals")}
                </span>
                <ScrollArea className="max-h-[26rem]">
                  <div className="flex flex-col gap-2 pr-2">
                    {proposals.map((proposal) => (
                      <ProposalCard
                        key={proposal.id}
                        proposal={proposal}
                        selected={selected[proposal.id] === true}
                        side={
                          sides[proposal.id] ??
                          proposal.suggestedCanonical
                        }
                        onSelectedChange={(value) =>
                          setSelected((current) => ({
                            ...current,
                            [proposal.id]: value,
                          }))
                        }
                        onSideChange={(side) =>
                          setSides((current) => ({
                            ...current,
                            [proposal.id]: side,
                          }))
                        }
                      />
                    ))}
                  </div>
                </ScrollArea>
              </div>
            )}

            {notes.length > 0 && (
              <div className="flex flex-col gap-2">
                <span className="text-sm font-medium">
                  {t("title.scanNotes")}
                </span>
                <ul className="flex flex-col gap-1 text-sm text-muted-foreground">
                  {notes.map((note) => (
                    <li key={note.id}>
                      {note.kind === "only_in"
                        ? t("title.scanNoteOnlyIn", {
                            number: note.numberText,
                            source: note.sourceName,
                          })
                        : t("title.scanNoteDuplicate", {
                            number: note.numberText,
                            source: note.sourceName,
                          })}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {errorText && (
              <Alert variant="destructive">
                <AlertTriangleIcon aria-hidden />
                <AlertTitle>{t("title.scanTitle")}</AlertTitle>
                <AlertDescription>{errorText}</AlertDescription>
              </Alert>
            )}
          </div>
        )}

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={apply.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("title.cancel")}
          </Button>
          <Button
            type="button"
            disabled={inputs.length === 0 || apply.isPending}
            onClick={submit}
          >
            {apply.isPending && <Spinner data-icon="inline-start" />}
            {t("title.scanApply", { count: inputs.length })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
