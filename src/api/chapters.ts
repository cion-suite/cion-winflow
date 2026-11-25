import { useQuery } from "@tanstack/react-query";

import type { ContentKind } from "@/lib/catalog";

import { ApiError, apiUrl, request } from "./client";

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
};

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
