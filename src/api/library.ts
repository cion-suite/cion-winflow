import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { LibraryStatus } from "@/lib/catalog";
import { titleQueryKey, type TitleSummary } from "./catalog";
import { jsonMutation, request } from "./client";

export type LibrarySort = "updated" | "added" | "title" | "chapters" | "rating";
export type LibraryOrder = "asc" | "desc";

export type LibraryItem = {
  title_id: number;
  status: LibraryStatus;
  score: number | null;
  added_at: number;
  updated_at: number;
  /** Эффективный прогресс: max(chapter_reads, imported_read_count). */
  read_count: number;
  /** Импортированная отметка прочитанного; 0, если импорта не было. */
  imported_read_count: number;
  /** Доступные главы предпочтительного источника: знаменатель карточки. */
  available_count: number;
  /** Глав в самом полном источнике: сортировка и экспорт. */
  chapter_count: number;
  /** Своя закладка как статус: перекрывает встроенный `status`. */
  custom_tab_id: number | null;
  title: TitleSummary;
};

/** Ответ upsert: запись без вложенной карточки тайтла. */
export type LibraryEntry = {
  title_id: number;
  status: LibraryStatus;
  score: number | null;
  custom_tab_id: number | null;
  added_at: number;
  updated_at: number;
};

export type StatusCounts = Record<LibraryStatus, number>;

export type LibraryCounts = {
  all: number;
  statuses: StatusCounts;
};

export type LibraryListResponse = {
  items: LibraryItem[];
  total: number;
  page: number;
  page_size: number;
  counts: LibraryCounts;
};

export type LibraryQuery = {
  page?: number;
  pageSize?: number;
  /** Поиск по названию только среди своих тайтлов. */
  q?: string;
  status?: LibraryStatus;
  tabId?: number;
  sort?: LibrarySort;
  order?: LibraryOrder;
};

export type LibraryInput = {
  status: LibraryStatus;
  score: number | null;
  /** null — встроенный статус, число — своя закладка вместо него. */
  custom_tab_id: number | null;
};

export type LibraryTab = {
  id: number;
  name: string;
  position: number;
  count: number;
};

export type LibraryTabsResponse = {
  tabs: LibraryTab[];
  limit: number;
};

export const LIBRARY_QUERY_KEY = ["library"] as const;
export const LIBRARY_TABS_QUERY_KEY = ["library-tabs"] as const;

function libraryPath(query: LibraryQuery): string {
  const params = new URLSearchParams();
  if (query.page && query.page > 1) params.set("page", String(query.page));
  if (query.pageSize) params.set("page_size", String(query.pageSize));
  if (query.q) params.set("q", query.q);
  if (query.status) params.set("status", query.status);
  if (query.tabId) params.set("tab_id", String(query.tabId));
  if (query.sort) params.set("sort", query.sort);
  if (query.order) params.set("order", query.order);

  const search = params.toString();
  return search ? `/library?${search}` : "/library";
}

function tabPath(tabId: number): string {
  return `/library/tabs/${tabId}`;
}

export const libraryApi = {
  list: (query: LibraryQuery) => request<LibraryListResponse>(libraryPath(query)),
  upsert: (titleId: number, input: LibraryInput) =>
    request<LibraryEntry>(`/library/${titleId}`, jsonMutation(input, "PUT")),
  remove: (titleId: number) =>
    request<void>(`/library/${titleId}`, { method: "DELETE" }),
  tabs: () => request<LibraryTabsResponse>("/library/tabs"),
  createTab: (name: string) =>
    request<LibraryTab>("/library/tabs", jsonMutation({ name })),
  renameTab: (tabId: number, name: string) =>
    request<LibraryTab>(tabPath(tabId), jsonMutation({ name }, "PUT")),
  deleteTab: (tabId: number) =>
    request<void>(tabPath(tabId), { method: "DELETE" }),
};

export function useLibraryQuery(
  query: LibraryQuery = {},
  options: { enabled?: boolean } = {},
) {
  return useQuery({
    queryKey: [
      ...LIBRARY_QUERY_KEY,
      query.q ?? "",
      query.status ?? "",
      query.tabId ?? null,
      query.sort ?? "updated",
      query.order ?? "desc",
      query.page ?? 1,
      query.pageSize ?? 50,
    ],
    queryFn: () => libraryApi.list(query),
    placeholderData: keepPreviousData,
    enabled: options.enabled ?? true,
  });
}

export function useLibraryTabsQuery() {
  return useQuery({
    queryKey: LIBRARY_TABS_QUERY_KEY,
    queryFn: libraryApi.tabs,
  });
}

function invalidateLibrary(
  queryClient: ReturnType<typeof useQueryClient>,
  titleId?: number,
): void {
  void queryClient.invalidateQueries({ queryKey: LIBRARY_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: LIBRARY_TABS_QUERY_KEY });
  if (titleId !== undefined) {
    void queryClient.invalidateQueries({ queryKey: titleQueryKey(titleId) });
  } else {
    // Удаление своей закладки меняет library.custom_tab_id у всех тайтлов
    // через ON DELETE SET NULL, поэтому кэш страницы тайтла тоже устарел.
    void queryClient.invalidateQueries({ queryKey: ["title"] });
  }
}

export function useUpsertLibraryMutation(titleId: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: LibraryInput) => libraryApi.upsert(titleId, input),
    onSuccess: () => invalidateLibrary(queryClient, titleId),
  });
}

export function useRemoveLibraryMutation(titleId: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => libraryApi.remove(titleId),
    onSuccess: () => invalidateLibrary(queryClient, titleId),
  });
}

export function useCreateLibraryTabMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name: string) => libraryApi.createTab(name),
    onSuccess: () => invalidateLibrary(queryClient),
  });
}

export function useRenameLibraryTabMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ tabId, name }: { tabId: number; name: string }) =>
      libraryApi.renameTab(tabId, name),
    onSuccess: () => invalidateLibrary(queryClient),
  });
}

export function useDeleteLibraryTabMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (tabId: number) => libraryApi.deleteTab(tabId),
    onSuccess: () => invalidateLibrary(queryClient),
  });
}
