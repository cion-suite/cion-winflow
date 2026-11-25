import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { jsonMutation, request, requestKeepalive } from "./client";

export type Progress = {
  chapter_key: string;
  page: number | null;
  seconds: number | null;
  scroll: number | null;
  updated_at: number;
};

export type ProgressResponse = {
  progress: Progress | null;
};

export type ProgressInput = {
  chapter_key: string;
  page?: number;
  seconds?: number;
  scroll?: number;
};

export function progressQueryKey(titleId: number | null) {
  return ["progress", titleId] as const;
}

export const progressApi = {
  get: (titleId: number) => request<ProgressResponse>(`/progress/${titleId}`),
  save: (titleId: number, input: ProgressInput) =>
    request<Progress>(`/progress/${titleId}`, jsonMutation(input, "PUT")),
  /** Сохранение при закрытии вкладки: ответ уже никого не интересует. */
  flush: (titleId: number, input: ProgressInput) =>
    requestKeepalive(`/progress/${titleId}`, jsonMutation(input, "PUT")),
};

export function useProgressQuery(titleId: number | null) {
  return useQuery({
    queryKey: progressQueryKey(titleId),
    queryFn: () => {
      if (titleId === null) throw new Error("title id is required");
      return progressApi.get(titleId);
    },
    enabled: titleId !== null,
    // Фоновый refetch не должен гоняться с keepalive-записью при скрытии вкладки
    refetchOnWindowFocus: false,
  });
}

export function useSaveProgressMutation(titleId: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: ProgressInput) => progressApi.save(titleId, input),
    onSuccess: (progress) => {
      // Локальный кэш обновляется без повторного запроса
      queryClient.setQueryData<ProgressResponse>(progressQueryKey(titleId), {
        progress,
      });
    },
  });
}
