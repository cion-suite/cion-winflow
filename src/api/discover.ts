import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { MediaKind, TitleStatus } from "@/lib/catalog";
import { apiUrl, jsonMutation, request } from "./client";
import { LIBRARY_QUERY_KEY, LIBRARY_TABS_QUERY_KEY } from "./library";

export type DiscoverItem = {
  code: string;
  source_name: string;
  external_id: string;
  kind: MediaKind;
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
  kind: MediaKind;
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
  search: (query: string, kind?: MediaKind) => {
    const params = new URLSearchParams({ q: query });
    if (kind) params.set("kind", kind);
    return request<DiscoverSearchResponse>(`/discover/search?${params.toString()}`);
  },
  add: (input: DiscoverAddInput) =>
    request<DiscoverAddResponse>(
      "/discover/add",
      jsonMutation({
        code: input.code,
        external_id: input.externalId,
        kind: input.kind,
      }),
    ),
};

export function useDiscoverSearchQuery(query: string, kind?: MediaKind) {
  return useQuery({
    queryKey: [...DISCOVER_QUERY_KEY, query, kind ?? ""],
    queryFn: () => discoverApi.search(query, kind),
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
