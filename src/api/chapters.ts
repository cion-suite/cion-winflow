import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import type { ContentKind } from "@/lib/catalog";

import { titleQueryKey, type TitleResponse } from "./catalog";
import { ApiError, apiUrl, jsonMutation, request } from "./client";

export type ContentPage = {
  id: number;
  index: number;
  width: number | null;
  height: number | null;
  /** Внутренний URL: браузер не ходит на чужие сайты за картинками. */
  image_url: string;
};

export type ChapterContent =
  | { kind: "images"; pages: ContentPage[] }
  | { kind: "video"; embed_url: string }
  | { kind: "text"; format: string };

export type ChapterSourceRef = {
  code: string;
  name: string;
};

export type ChapterContentResponse = {
  chapter_id: number;
  chapter_key: string;
  number_text: string | null;
  name: string | null;
  content_kind: ContentKind;
  source: ChapterSourceRef;
  content: ChapterContent;
  fallback?: {
    from_source: ChapterSourceRef;
    reason: string;
  };
};

export type ChapterReadsInput = {
  chapter_keys: string[];
  read: boolean;
};

export type ChapterReadsResponse = {
  read_keys: string[];
};

export function chapterContentQueryKey(
  titleId: number | null,
  chapterKey: string | null,
) {
  return ["chapter-content", titleId, chapterKey] as const;
}

export const chaptersApi = {
  content: (titleId: number, chapterKey: string) =>
    request<ChapterContentResponse>(
      `/titles/${titleId}/chapters/${encodeURIComponent(chapterKey)}/content`,
    ),
  /** Ручная отметка глав: журнал chapter_reads, прогресс чтения не меняется. */
  setRead: (titleId: number, input: ChapterReadsInput) =>
    request<ChapterReadsResponse>(
      `/titles/${titleId}/chapters/read`,
      jsonMutation(input, "PUT"),
    ),
};

/**
 * Сервер принимает не больше 2000 ключей за запрос, а «эта и все предыдущие»
 * на длинном тайтле может быть больше. Пачка в 500 оставляет запас и держит
 * каждый запрос атомарным.
 */
const READ_BATCH_SIZE = 500;

export function useSetChapterReadMutation(titleId: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: ChapterReadsInput) => {
      const keys = Array.from(new Set(input.chapter_keys));
      if (keys.length <= READ_BATCH_SIZE) {
        return chaptersApi.setRead(titleId, { ...input, chapter_keys: keys });
      }

      let response: ChapterReadsResponse = { read_keys: [] };
      for (let start = 0; start < keys.length; start += READ_BATCH_SIZE) {
        response = await chaptersApi.setRead(titleId, {
          ...input,
          chapter_keys: keys.slice(start, start + READ_BATCH_SIZE),
        });
      }
      return response;
    },
    onSuccess: (data) => {
      // Ответ — полный список ключей: страница тайтла обновляется без refetch
      queryClient.setQueryData<TitleResponse>(titleQueryKey(titleId), (current) =>
        current ? { ...current, read_keys: data.read_keys } : current,
      );
      // read_count карточки библиотеки считается по chapter_reads
      void queryClient.invalidateQueries({ queryKey: ["library"] });
    },
  });
}

export function useChapterContentQuery(
  titleId: number | null,
  chapterKey: string | null,
) {
  return useQuery({
    queryKey: chapterContentQueryKey(titleId, chapterKey),
    queryFn: () => {
      if (titleId === null || chapterKey === null) {
        throw new Error("title id and chapter key are required");
      }
      return chaptersApi.content(titleId, chapterKey);
    },
    enabled: titleId !== null && chapterKey !== null,
    // Список страниц меняется редко: повторный показ не должен ходить в сеть
    staleTime: 60_000,
    // 403/404 — окончательный ответ: повторять их только задерживает ошибку
    retry: (failureCount, error) =>
      failureCount < 2 &&
      !(error instanceof ApiError && [403, 404, 409].includes(error.status)),
  });
}

export function pageImageUrl(pageId: number): string {
  return apiUrl(`/pages/${pageId}/image`);
}
