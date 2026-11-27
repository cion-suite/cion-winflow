import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ME_QUERY_KEY, type Role } from "./auth";
import { apiUrl, request } from "./client";
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

export const PROFILE_QUERY_KEY = ["profile"] as const;

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
