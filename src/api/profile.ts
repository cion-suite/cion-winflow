import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ME_QUERY_KEY, useAuth, type Role } from "./auth";
import { apiUrl, request, requestBlob } from "./client";
import type { LibraryStatus } from "@/lib/catalog";
import type { LibraryCounts, LibraryItem } from "./library";

export type ProfileUser = {
  id: number;
  username: string;
  role: Role;
  created_at: number;
  /** NULL — аватара нет; иначе время загрузки. */
  avatar_updated_at: number | null;
  /** Монотонная версия аватара для ETag и cache-buster. */
  avatar_version: number;
};

export type ProfileStats = {
  /** Глав отмечено прочитанными по журналу chapter_reads. */
  read_chapters: number;
  /** Счётчики библиотеки без «Не интересно». */
  counts: LibraryCounts;
};

export type ProfileLibrary = {
  items: LibraryItem[];
  total: number;
  page: number;
  page_size: number;
};

export type ProfileResponse = {
  user: ProfileUser;
  stats: ProfileStats;
  library: ProfileLibrary;
};

export type AvatarResponse = {
  avatar_updated_at: number;
  avatar_version: number;
};

export type ProfileQuery = {
  status?: LibraryStatus;
  page?: number;
  pageSize?: number;
};

export type BookmarkImportState = "running" | "completed" | "failed";

export type BookmarkImportError = {
  name: string;
  message: string;
  /** Машинный код ошибки API для локализации. */
  code: string;
};

/** Фоновое задание импорта закладок: профиль опрашивает его, пока running. */
export type BookmarkImportJob = {
  id: string;
  state: BookmarkImportState;
  total: number;
  processed: number;
  succeeded: number;
  failed: number;
  /** Новые тайтлы, которых не было на сервере. */
  titles_imported: number;
  library_created: number;
  library_updated: number;
  read_marked: number;
  errors: BookmarkImportError[];
  errors_truncated: boolean;
  started_at: number;
  finished_at: number | null;
};

export type BookmarkImportResponse = {
  job: BookmarkImportJob | null;
};

export const PROFILE_QUERY_KEY = ["profile"] as const;
export const BOOKMARK_IMPORT_QUERY_KEY = ["bookmark-import"] as const;

/** URL аватара: версия в query делает ETag и кэш браузера корректными. */
export function userAvatarUrl(
  userId: number,
  avatarUpdatedAt: number | null | undefined,
  avatarVersion: number,
): string | null {
  // undefined — ответ старого сервера без поля: лучше инициал, чем ?v=undefined
  if (avatarUpdatedAt === null || avatarUpdatedAt === undefined) return null;
  return `${apiUrl(`/users/${userId}/avatar`)}?v=${avatarVersion}`;
}

function profilePath(userId: number, query: ProfileQuery): string {
  const params = new URLSearchParams();
  if (query.status) params.set("status", query.status);
  if (query.page && query.page > 1) params.set("page", String(query.page));
  if (query.pageSize) params.set("page_size", String(query.pageSize));

  const search = params.toString();
  return search ? `/users/${userId}/profile?${search}` : `/users/${userId}/profile`;
}

export const profileApi = {
  get: (userId: number, query: ProfileQuery) =>
    request<ProfileResponse>(profilePath(userId, query)),
  uploadAvatar: (file: File) => {
    const body = new FormData();
    body.append("file", file);
    return request<AvatarResponse>("/me/avatar", { method: "POST", body });
  },
  deleteAvatar: () => request<void>("/me/avatar", { method: "DELETE" }),
  latestBookmarkImport: () =>
    request<BookmarkImportResponse>("/me/import/bookmarks"),
  startBookmarkImport: (file: File) => {
    const body = new FormData();
    body.append("file", file);
    return request<{ job: BookmarkImportJob }>("/me/import/bookmarks", {
      method: "POST",
      body,
    });
  },
  exportBookmarks: () => requestBlob("/me/export/bookmarks"),
};

export function useProfileQuery(
  userId: number | null,
  query: ProfileQuery = {},
  options: { enabled?: boolean } = {},
) {
  return useQuery({
    queryKey: [
      ...PROFILE_QUERY_KEY,
      userId ?? 0,
      query.status ?? "",
      query.page ?? 1,
      query.pageSize ?? 12,
    ],
    queryFn: () => {
      // enabled=false не даёт вызвать запрос без пользователя
      if (userId === null) throw new Error("profile query needs a user id");
      return profileApi.get(userId, query);
    },
    // Держим прошлую страницу при смене вкладки, но не при смене uid:
    // иначе на новом профиле на миг показались бы чужие ник и библиотека.
    placeholderData: (previousData, previousQuery) =>
      previousQuery?.queryKey[1] === (userId ?? 0) ? previousData : undefined,
    enabled: (options.enabled ?? true) && userId !== null,
  });
}

function invalidateAvatar(
  queryClient: ReturnType<typeof useQueryClient>,
): void {
  void queryClient.invalidateQueries({ queryKey: ME_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: PROFILE_QUERY_KEY });
}

export function useUploadAvatarMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: profileApi.uploadAvatar,
    onSuccess: () => invalidateAvatar(queryClient),
  });
}

export function useDeleteAvatarMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: profileApi.deleteAvatar,
    onSuccess: () => invalidateAvatar(queryClient),
  });
}

/** Последнее задание импорта; пока оно running, опрос идёт каждые 1.5 с. */
export function useBookmarkImportQuery() {
  const { user } = useAuth();
  return useQuery({
    // uid в ключе: после смены пользователя чужой импорт не показывается
    queryKey: [...BOOKMARK_IMPORT_QUERY_KEY, user?.id ?? 0],
    queryFn: profileApi.latestBookmarkImport,
    enabled: user !== null,
    refetchInterval: (query) =>
      query.state.data?.job?.state === "running" ? 1500 : false,
  });
}

export function useBookmarkImportMutation() {
  const queryClient = useQueryClient();
  const { user } = useAuth();

  return useMutation({
    mutationFn: profileApi.startBookmarkImport,
    onSuccess: (data) => {
      // Сразу показываем своё задание, не дожидаясь refetch: иначе диалог
      // на мгновение покажет прошлый результат или пустую форму.
      queryClient.setQueryData([...BOOKMARK_IMPORT_QUERY_KEY, user?.id ?? 0], data);
      void queryClient.invalidateQueries({
        queryKey: BOOKMARK_IMPORT_QUERY_KEY,
      });
    },
  });
}

/** Скачивание экспорта: возвращает Blob, ошибки — обычный ApiError. */
export function useExportBookmarksMutation() {
  return useMutation({
    mutationFn: profileApi.exportBookmarks,
  });
}

/** Импорт меняет сразу библиотеку, профиль и прогресс всех затронутых тайтлов. */
export function invalidateAfterBookmarkImport(
  queryClient: ReturnType<typeof useQueryClient>,
): void {
  void queryClient.invalidateQueries({ queryKey: PROFILE_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: ["library"] });
  void queryClient.invalidateQueries({ queryKey: ["library-tabs"] });
  void queryClient.invalidateQueries({ queryKey: ["title"] });
  void queryClient.invalidateQueries({ queryKey: ["chapter-content"] });
  void queryClient.invalidateQueries({ queryKey: ["progress"] });
}
