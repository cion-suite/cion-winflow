import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { TitleKind, TitleStatus } from "@/lib/catalog";
import { apiUrl, jsonMutation, request } from "./client";
import { LIBRARY_QUERY_KEY, LIBRARY_TABS_QUERY_KEY } from "./library";

export type DiscoverItem = {
  code: string;
  source_name: string;
  external_id: string;
  /** Тип из выдачи источника, если он его отдаёт. */
  title_kind: TitleKind | null;
  name: string;
  alt_names: string[];
  cover_ref: string | null;
  year: number | null;
  status: TitleStatus | null;
  /** Тайтл уже импортирован на сервер. */
  title_id: number | null;
  in_library: boolean;
};

export type DiscoverSourceError = {
  code: string;
  source_name: string;
  message: string;
};

export type DiscoverSearchResponse = {
  items: DiscoverItem[];
  errors: DiscoverSourceError[];
};

export type DiscoverAddInput = {
  code: string;
  externalId: string;
};

export type DiscoverAddResponse = {
  title_id: number;
  title_created: boolean;
  imported_chapters: number;
  library_created: boolean;
  status: string;
};

export const DISCOVER_QUERY_KEY = ["discover"] as const;

/** Обложка из выдачи источника идёт через серверный прокси, не напрямую. */
export function sourceCoverUrl(code: string, reference: string): string {
  const params = new URLSearchParams({ ref: reference });
  return apiUrl(`/sources/${encodeURIComponent(code)}/cover?${params.toString()}`);
}

export const discoverApi = {
  search: (query: string) => {
    const params = new URLSearchParams({ q: query });
    return request<DiscoverSearchResponse>(`/discover/search?${params.toString()}`);
  },
  add: (input: DiscoverAddInput) =>
    request<DiscoverAddResponse>(
      "/discover/add",
      jsonMutation({
        code: input.code,
        external_id: input.externalId,
      }),
    ),
};

export function useDiscoverSearchQuery(query: string) {
  return useQuery({
    queryKey: [...DISCOVER_QUERY_KEY, query],
    queryFn: () => discoverApi.search(query),
    enabled: query.trim().length > 0,
    placeholderData: keepPreviousData,
    // Выдача источника меняется медленно: не дёргаем сеть при каждом фокусе.
    staleTime: 30_000,
  });
}

export function useAddDiscoveredTitleMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: discoverApi.add,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: LIBRARY_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: LIBRARY_TABS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: ["title"] });
      void queryClient.invalidateQueries({ queryKey: DISCOVER_QUERY_KEY });
    },
  });
}
