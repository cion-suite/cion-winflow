import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import {
  joinList,
  splitList,
  type Availability,
  type ContentKind,
  type CoverSize,
  type LibraryStatus,
  type MediaKind,
  type TitleFormValues,
  type TitleStatus,
} from "@/lib/catalog";
import { apiUrl, jsonMutation, request } from "./client";

export type TitleSummary = {
  id: number;
  kind: MediaKind;
  name: string;
  year: number | null;
  status: TitleStatus | null;
  rating: number | null;
  has_cover: boolean;
  updated_at: number;
};

export type Title = TitleSummary & {
  alt_names: string[];
  authors: string[];
  genres: string[];
  description: string | null;
  lang: string | null;
  /** Какие размеры обложки реально сохранены на сервере. */
  cover_sizes: CoverSize[];
  origin_source_id: number | null;
  preferred_source_id: number | null;
  created_at: number;
};

export type LibraryState = {
  status: LibraryStatus;
  score: number | null;
  favorite: boolean;
  tab_ids: number[];
};

export type ChapterVariant = {
  chapter_id: number;
  /** Собственный ключ варианта: может отличаться от canonical. */
  chapter_key: string;
  source_id: number;
  source_code: string;
  source_name: string;
  group_name: string | null;
  language: string | null;
  availability: Availability;
  is_locked: boolean;
  content_kind: ContentKind | null;
  page_count: number | null;
  source_enabled: boolean;
};

export type TitleChapter = {
  chapter_key: string;
  number_text: string | null;
  number_milli: number | null;
  volume: number | null;
  chapter_kind: string;
  name: string | null;
  content_kind: ContentKind | null;
  page_count: number | null;
  availability: Availability;
  is_locked: boolean;
  published_at: number | null;
  selected_chapter_id: number;
  variants: ChapterVariant[];
};

export type TitleSource = {
  source_id: number;
  source_entry_id: number;
  code: string;
  name: string;
  enabled: boolean;
  is_primary: boolean;
  chapter_count: number;
  available_count: number;
  locked_count: number;
  missing_count: number;
  stats_updated_at: number | null;
  last_error: string | null;
};

export type ChapterAlias = {
  alias_key: string;
  canonical_key: string;
};

export type TitleResponse = {
  title: Title;
  library: LibraryState | null;
  /** Только БД: сервер не ходит в источники при показе тайтла. */
  chapters: TitleChapter[];
  sources: TitleSource[];
  aliases: ChapterAlias[];
  read_keys: string[];
};

export type TitlePayload = {
  title: Title;
};

export type CoverResponse = {
  has_cover: boolean;
  updated_at: number;
};

/** Полная замена: PUT присылает все поля, отсутствующие — пустые. */
export type TitleInput = {
  name: string;
  kind: MediaKind;
  alt_names: string[];
  authors: string[];
  genres: string[];
  description: string | null;
  status: TitleStatus | null;
  year: number | null;
  lang: string | null;
  rating: number | null;
};

export function titleQueryKey(id: number | null) {
  return ["title", id] as const;
}

/** Адрес обложки нужного размера: после замены файла URL меняется. */
export function titleCoverUrl(
  title: Pick<TitleSummary, "id" | "updated_at">,
  size: CoverSize,
): string {
  return apiUrl(`/titles/${title.id}/cover?size=${size}&v=${title.updated_at}`);
}

export const catalogApi = {
  title: (id: number) => request<TitleResponse>(`/titles/${id}`),
  create: (input: TitleInput) => request<TitlePayload>("/titles", jsonMutation(input)),
  update: (id: number, input: TitleInput) =>
    request<TitlePayload>(`/titles/${id}`, jsonMutation(input, "PUT")),
  uploadCover: (id: number, size: CoverSize, file: File) => {
    const body = new FormData();
    body.append("file", file);
    return request<CoverResponse>(`/titles/${id}/cover?size=${size}`, {
      method: "POST",
      body,
    });
  },
  deleteCover: (id: number, size: CoverSize) =>
    request<CoverResponse>(`/titles/${id}/cover?size=${size}`, { method: "DELETE" }),
};

export function useTitleQuery(id: number | null) {
  return useQuery({
    queryKey: titleQueryKey(id),
    queryFn: () => {
      if (id === null) throw new Error("title id is required");
      return catalogApi.title(id);
    },
    enabled: id !== null,
  });
}

export function useCreateTitleMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: catalogApi.create,
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["title"] }),
  });
}

export function useUpdateTitleMutation(id: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: TitleInput) => catalogApi.update(id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: titleQueryKey(id) });
    },
  });
}

export type UploadCoverInput = {
  size: CoverSize;
  file: File;
};

/** Обложки меняют `has_cover` и `updated_at`, поэтому кэш библиотеки тоже гаснет. */
function invalidateCoverQueries(queryClient: QueryClient, id: number) {
  void queryClient.invalidateQueries({ queryKey: titleQueryKey(id) });
  void queryClient.invalidateQueries({ queryKey: ["library"] });
}

export function useUploadCoverMutation(id: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ size, file }: UploadCoverInput) =>
      catalogApi.uploadCover(id, size, file),
    onSuccess: () => invalidateCoverQueries(queryClient, id),
  });
}

export function useDeleteCoverMutation(id: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (size: CoverSize) => catalogApi.deleteCover(id, size),
    onSuccess: () => invalidateCoverQueries(queryClient, id),
  });
}

export function titleToFormValues(title: Title): TitleFormValues {
  return {
    name: title.name,
    kind: title.kind,
    altNames: joinList(title.alt_names),
    authors: joinList(title.authors),
    genres: joinList(title.genres),
    description: title.description ?? "",
    status: title.status ?? "",
    year: title.year === null ? "" : String(title.year),
    lang: title.lang ?? "",
    rating: title.rating === null ? "" : String(title.rating),
  };
}

export function formValuesToTitleInput(values: TitleFormValues): TitleInput {
  return {
    name: values.name.trim(),
    kind: values.kind,
    alt_names: splitList(values.altNames),
    authors: splitList(values.authors),
    genres: splitList(values.genres),
    description: values.description.trim() === "" ? null : values.description.trim(),
    status: values.status === "" ? null : values.status,
    year: values.year.trim() === "" ? null : Number(values.year),
    lang: values.lang.trim() === "" ? null : values.lang.trim(),
    rating: values.rating.trim() === "" ? null : Number(values.rating),
  };
}
