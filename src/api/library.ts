import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { LibraryStatus } from "@/lib/catalog";
import { titleQueryKey, type TitleSummary } from "./catalog";
import { jsonMutation, request } from "./client";

export type LibraryItem = {
  title_id: number;
  status: LibraryStatus;
  score: number | null;
  added_at: number;
  updated_at: number;
  title: TitleSummary;
};

/** Ответ upsert: запись без вложенной карточки тайтла. */
export type LibraryEntry = {
  title_id: number;
  status: LibraryStatus;
  score: number | null;
  added_at: number;
  updated_at: number;
};

export type LibraryListResponse = {
  items: LibraryItem[];
  total: number;
  page: number;
  page_size: number;
};

export type LibraryQuery = {
  page?: number;
  pageSize?: number;
};

export type LibraryInput = {
  status: LibraryStatus;
  score: number | null;
};

export const LIBRARY_QUERY_KEY = ["library"] as const;

function libraryPath(query: LibraryQuery): string {
  const params = new URLSearchParams();
  if (query.page && query.page > 1) params.set("page", String(query.page));
  if (query.pageSize) params.set("page_size", String(query.pageSize));

  const search = params.toString();
  return search ? `/library?${search}` : "/library";
}

export const libraryApi = {
  list: (query: LibraryQuery) => request<LibraryListResponse>(libraryPath(query)),
  upsert: (titleId: number, input: LibraryInput) =>
    request<LibraryEntry>(`/library/${titleId}`, jsonMutation(input, "PUT")),
  remove: (titleId: number) =>
    request<void>(`/library/${titleId}`, { method: "DELETE" }),
};

export function useLibraryQuery(query: LibraryQuery = {}) {
  return useQuery({
    queryKey: [...LIBRARY_QUERY_KEY, query.page ?? 1, query.pageSize ?? 50],
    queryFn: () => libraryApi.list(query),
    placeholderData: keepPreviousData,
  });
}

export function useUpsertLibraryMutation(titleId: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: LibraryInput) => libraryApi.upsert(titleId, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: LIBRARY_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: titleQueryKey(titleId) });
    },
  });
}

export function useRemoveLibraryMutation(titleId: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => libraryApi.remove(titleId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: LIBRARY_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: titleQueryKey(titleId) });
    },
  });
}
