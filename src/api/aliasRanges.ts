import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { jsonMutation, request } from "./client";
import { progressQueryKey } from "./progress";
import { titleQueryKey } from "./catalog";

export type AliasRangeStatus = "active" | "needs_review" | "broken";
export type AliasRangeMappingMode = "volume_number" | "number_only";

export type AliasRangeAnchor = {
  chapter_id: number | null;
  chapter_key: string;
  volume: number | null;
  number_text: string | null;
  number_milli: number | null;
};

export type AliasRangeCoverage = {
  linked: number;
  pending: number;
};

export type AliasRange = {
  id: number;
  source_entry_id: number;
  source_id: number;
  source_code: string;
  source_name: string;
  anchor: AliasRangeAnchor;
  canonical: AliasRangeAnchor;
  chapter_kind: string;
  mapping_mode: AliasRangeMappingMode;
  status: AliasRangeStatus;
  coverage: AliasRangeCoverage;
  next_range_id: number | null;
  warnings: string[];
};

export type AliasManualLink = {
  alias_key: string;
  canonical_key: string;
};

export type AliasRangeListResponse = {
  ranges: AliasRange[];
  /** Ручные пары вне диапазонов: одиночные или с разным смещением. */
  manual_aliases: AliasManualLink[];
};

export type AliasRangeInput = {
  source_entry_id: number;
  anchor_chapter_id: number;
  canonical_chapter_id: number;
};

export type AliasRangeBatchInput = {
  ranges: AliasRangeInput[];
};

const rangesQueryKey = (titleId: number) => ["alias-ranges", titleId] as const;

function rangePath(titleId: number, rangeId?: number): string {
  const base = `/titles/${titleId}/alias-ranges`;
  return rangeId === undefined ? base : `${base}/${rangeId}`;
}

export const aliasRangesApi = {
  list: (titleId: number) =>
    request<AliasRangeListResponse>(rangePath(titleId)),
  create: (titleId: number, input: AliasRangeInput) =>
    request<AliasRange>(rangePath(titleId), jsonMutation(input)),
  update: (titleId: number, rangeId: number, input: AliasRangeInput) =>
    request<AliasRange>(rangePath(titleId, rangeId), jsonMutation(input, "PUT")),
  remove: (titleId: number, rangeId: number) =>
    request<undefined>(rangePath(titleId, rangeId), { method: "DELETE" }),
  apply: (titleId: number, ranges: AliasRangeInput[]) =>
    request<AliasRangeListResponse>(
      `${rangePath(titleId)}/apply`,
      jsonMutation({ ranges } satisfies AliasRangeBatchInput),
    ),
};

export function useAliasRangesQuery(titleId: number, enabled = true) {
  return useQuery({
    queryKey: rangesQueryKey(titleId),
    queryFn: () => aliasRangesApi.list(titleId),
    enabled,
  });
}

function invalidateAfterRanges(
  queryClient: ReturnType<typeof useQueryClient>,
  titleId: number,
): void {
  void queryClient.invalidateQueries({ queryKey: rangesQueryKey(titleId) });
  void queryClient.invalidateQueries({ queryKey: titleQueryKey(titleId) });
  void queryClient.invalidateQueries({ queryKey: ["chapter-content", titleId] });
  void queryClient.invalidateQueries({ queryKey: progressQueryKey(titleId) });
  void queryClient.invalidateQueries({ queryKey: ["library"] });
}

export function useCreateAliasRangeMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: AliasRangeInput) => aliasRangesApi.create(titleId, input),
    onSuccess: () => invalidateAfterRanges(queryClient, titleId),
  });
}

export function useUpdateAliasRangeMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ rangeId, input }: { rangeId: number; input: AliasRangeInput }) =>
      aliasRangesApi.update(titleId, rangeId, input),
    onSuccess: () => invalidateAfterRanges(queryClient, titleId),
  });
}

export function useDeleteAliasRangeMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (rangeId: number) => aliasRangesApi.remove(titleId, rangeId),
    onSuccess: () => invalidateAfterRanges(queryClient, titleId),
  });
}

export function useApplyAliasRangesMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (ranges: AliasRangeInput[]) => aliasRangesApi.apply(titleId, ranges),
    onSuccess: () => invalidateAfterRanges(queryClient, titleId),
  });
}
