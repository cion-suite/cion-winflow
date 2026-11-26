import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { TitleKind, TitleStatus } from "@/lib/catalog";
import { jsonMutation, request } from "./client";
import { titleQueryKey, type ChapterAlias } from "./catalog";
import { progressQueryKey } from "./progress";

export type SourceState = {
  code: string;
  name: string;
  /** Сайт для подписи: у Remanga отличается от API-хоста base_url. */
  site_url: string;
  /** Адрес, который подставляется при первом включении источника. */
  default_base_url: string;
  /** Языковая колонка админки: ru или en. */
  lang: string;
  version: number;
  capabilities: string[];
  config_schema: unknown[];
  filters_schema: unknown[];
  configured: boolean;
  enabled: boolean;
  base_url: string | null;
  config: unknown;
  config_error: string | null;
  last_checked_at: number | null;
  last_error: string | null;
};

export type SourceListResponse = {
  sources: SourceState[];
};

export type SourceSearchItem = {
  external_id: string;
  /** Тип из выдачи источника, если он его отдаёт. */
  kind: TitleKind | null;
  name: string;
  alt_names: string[];
  cover_ref: string | null;
  year: number | null;
  status: TitleStatus | null;
  external_url: string | null;
};

export type SourceSearchResponse = {
  items: SourceSearchItem[];
  next_cursor: string | null;
  total: number | null;
};

export type SourceSearchInput = {
  query: string;
  cursor?: string;
  filters?: unknown;
};

export type SourcePriority = {
  source_id: number;
  code: string;
  name: string;
  enabled: boolean;
  rank: number | null;
};

export type SourcePrioritiesResponse = {
  lang: string;
  sources: SourcePriority[];
};

export type AttachSourceInput = {
  code: string;
  external_id: string;
};

export type AttachSourceResponse = {
  title_id: number;
  source_entry_id: number;
  title_created: boolean;
  imported_chapters: number;
  stats: {
    chapter_count: number;
    available_count: number;
    locked_count: number;
    missing_count: number;
  };
};

export type AliasListResponse = {
  aliases: ChapterAlias[];
};

export type RefreshEntry = {
  source_entry_id: number;
  code: string;
  status: "ok" | "error";
  imported_chapters: number;
  error: string | null;
};

export type RefreshResponse = {
  title_id: number;
  entries: RefreshEntry[];
  preferred_source_id: number | null;
};

export type ProbeResponse = {
  status: "ok" | "auth" | "cloudflare" | "parse" | "error";
  message: string | null;
  checked_at: number;
};

export const SOURCES_QUERY_KEY = ["sources"] as const;
export const SOURCE_PRIORITIES_QUERY_KEY = ["source-priorities"] as const;

function sourcePath(code: string): string {
  return `/sources/${encodeURIComponent(code)}`;
}

function aliasPath(titleId: number, aliasKey: string): string {
  return `/titles/${titleId}/chapter-aliases/${encodeURIComponent(aliasKey)}`;
}

export const adminApi = {
  sources: () => request<SourceListResponse>("/sources"),
  toggleSource: (code: string, baseUrl: string | null, enabled: boolean) =>
    request<SourceState>(sourcePath(code), jsonMutation({ base_url: baseUrl, enabled }, "PUT")),
  probe: (code: string) => request<ProbeResponse>(`${sourcePath(code)}/probe`, jsonMutation({})),
  search: (code: string, input: SourceSearchInput) =>
    request<SourceSearchResponse>(`${sourcePath(code)}/search`, jsonMutation(input)),
  priorities: (lang: string) => {
    const params = new URLSearchParams({ lang });
    return request<SourcePrioritiesResponse>(`/admin/source-priorities?${params.toString()}`);
  },
  savePriorities: (lang: string, sourceCodes: string[]) =>
    request<SourcePrioritiesResponse>(
      "/admin/source-priorities",
      jsonMutation({ lang, source_codes: sourceCodes }, "PUT"),
    ),
  attach: (titleId: number, input: AttachSourceInput) =>
    request<AttachSourceResponse>(`/titles/${titleId}/sources`, jsonMutation(input)),
  detach: (titleId: number, entryId: number) =>
    request<undefined>(`/titles/${titleId}/sources/${entryId}`, { method: "DELETE" }),
  primary: (titleId: number, entryId: number) =>
    request<undefined>(`/titles/${titleId}/primary-source`, jsonMutation({ source_entry_id: entryId }, "PUT")),
  aliases: (titleId: number) => request<AliasListResponse>(`/titles/${titleId}/chapter-aliases`),
  addAlias: (titleId: number, aliasKey: string, canonicalKey: string) =>
    request<ChapterAlias>(
      `/titles/${titleId}/chapter-aliases`,
      jsonMutation({ alias_key: aliasKey, canonical_key: canonicalKey }),
    ),
  deleteAlias: (titleId: number, aliasKey: string) =>
    request<undefined>(aliasPath(titleId, aliasKey), { method: "DELETE" }),
  refresh: (titleId: number) =>
    request<RefreshResponse>(`/admin/refresh/${titleId}`, jsonMutation({})),
};

export function useSourcesQuery() {
  return useQuery({
    queryKey: SOURCES_QUERY_KEY,
    queryFn: adminApi.sources,
  });
}

export function useSourcePrioritiesQuery(lang: string) {
  return useQuery({
    queryKey: [...SOURCE_PRIORITIES_QUERY_KEY, lang],
    queryFn: () => adminApi.priorities(lang),
  });
}

export function useToggleSourceMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      code,
      baseUrl,
      enabled,
    }: {
      code: string;
      baseUrl: string | null;
      enabled: boolean;
    }) => adminApi.toggleSource(code, baseUrl, enabled),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SOURCES_QUERY_KEY });
      // Новый источник появляется в списке приоритетов после настройки.
      void queryClient.invalidateQueries({ queryKey: SOURCE_PRIORITIES_QUERY_KEY });
      // Выключение меняет preferred source и выбранный вариант главы.
      void queryClient.invalidateQueries({ queryKey: ["title"] });
      void queryClient.invalidateQueries({ queryKey: ["chapter-content"] });
      void queryClient.invalidateQueries({ queryKey: ["progress"] });
    },
  });
}

export function useProbeSourceMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (code: string) => adminApi.probe(code),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SOURCES_QUERY_KEY }),
  });
}

export function useSourceSearchMutation() {
  return useMutation({
    mutationFn: ({ code, input }: { code: string; input: SourceSearchInput }) =>
      adminApi.search(code, input),
  });
}

export function useSaveSourcePrioritiesMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ lang, sourceCodes }: { lang: string; sourceCodes: string[] }) =>
      adminApi.savePriorities(lang, sourceCodes),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SOURCE_PRIORITIES_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: ["title"] });
      void queryClient.invalidateQueries({ queryKey: ["chapter-content"] });
      void queryClient.invalidateQueries({ queryKey: ["progress"] });
    },
  });
}

function invalidateTitle(queryClient: ReturnType<typeof useQueryClient>, titleId: number): void {
  void queryClient.invalidateQueries({ queryKey: titleQueryKey(titleId) });
  // Связи и доступность меняют выбранный вариант и canonical-ключ прогресса.
  void queryClient.invalidateQueries({ queryKey: ["chapter-content", titleId] });
  void queryClient.invalidateQueries({ queryKey: progressQueryKey(titleId) });
  // Attach/refresh/import могут добрать обложки: has_cover и updated_at
  // видны в карточках библиотеки и поиске по шапке.
  void queryClient.invalidateQueries({ queryKey: ["library"] });
}

export function useAttachSourceMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: AttachSourceInput) => adminApi.attach(titleId, input),
    onSuccess: () => invalidateTitle(queryClient, titleId),
  });
}

export function useDetachSourceMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (entryId: number) => adminApi.detach(titleId, entryId),
    onSuccess: () => invalidateTitle(queryClient, titleId),
  });
}

export function useSetPrimarySourceMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (entryId: number) => adminApi.primary(titleId, entryId),
    onSuccess: () => invalidateTitle(queryClient, titleId),
  });
}

export function useAddAliasMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ aliasKey, canonicalKey }: { aliasKey: string; canonicalKey: string }) =>
      adminApi.addAlias(titleId, aliasKey, canonicalKey),
    onSuccess: () => invalidateTitle(queryClient, titleId),
  });
}

export function useDeleteAliasMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (aliasKey: string) => adminApi.deleteAlias(titleId, aliasKey),
    onSuccess: () => invalidateTitle(queryClient, titleId),
  });
}

export function useRefreshTitleMutation(titleId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => adminApi.refresh(titleId),
    onSuccess: () => invalidateTitle(queryClient, titleId),
  });
}
