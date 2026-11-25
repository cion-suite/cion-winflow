import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  joinList,
  splitList,
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
  origin_source_id: number | null;
  preferred_source_id: number | null;
  created_at: number;
};

export type LibraryState = {
  status: LibraryStatus;
  score: number | null;
};

export type TitleResponse = {
  title: Title;
  library: LibraryState | null;
};

export type TitlePayload = {
  title: Title;
};

export type CoverResponse = {
  has_cover: boolean;
  updated_at: number;
};

export type CatalogResponse = {
  items: TitleSummary[];
  total: number;
  page: number;
  page_size: number;
};

export type CatalogQuery = {
  q?: string;
  kind?: MediaKind;
  page?: number;
  pageSize?: number;
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

export const CATALOG_QUERY_KEY = ["catalog"] as const;

function catalogPath(query: CatalogQuery): string {
  const params = new URLSearchParams();
  if (query.q) params.set("q", query.q);
  if (query.kind) params.set("kind", query.kind);
  if (query.page && query.page > 1) params.set("page", String(query.page));
  if (query.pageSize) params.set("page_size", String(query.pageSize));

  const search = params.toString();
  return search ? `/catalog?${search}` : "/catalog";
}

export function titleQueryKey(id: number | null) {
  return ["title", id] as const;
}

/** Адрес обложки с версией: после замены файла URL меняется. */
export function titleCoverUrl(
  title: Pick<TitleSummary, "id" | "updated_at">,
  size: "thumb" | "full",
): string {
  return apiUrl(`/titles/${title.id}/cover?size=${size}&v=${title.updated_at}`);
}

export const catalogApi = {
  list: (query: CatalogQuery) => request<CatalogResponse>(catalogPath(query)),
  title: (id: number) => request<TitleResponse>(`/titles/${id}`),
  create: (input: TitleInput) => request<TitlePayload>("/titles", jsonMutation(input)),
  update: (id: number, input: TitleInput) =>
    request<TitlePayload>(`/titles/${id}`, jsonMutation(input, "PUT")),
  uploadCover: (id: number, file: File) => {
    const body = new FormData();
    body.append("file", file);
    return request<CoverResponse>(`/titles/${id}/cover`, { method: "POST", body });
  },
};

export function useCatalogQuery(query: CatalogQuery) {
  return useQuery({
    queryKey: [
      ...CATALOG_QUERY_KEY,
      query.q ?? "",
      query.kind ?? "",
      query.page ?? 1,
      query.pageSize ?? 50,
    ],
    queryFn: () => catalogApi.list(query),
    // При смене страницы прошлая выдача остаётся на экране без мигания
    placeholderData: keepPreviousData,
  });
}

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
    onSuccess: () => queryClient.invalidateQueries({ queryKey: CATALOG_QUERY_KEY }),
  });
}

export function useUpdateTitleMutation(id: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: TitleInput) => catalogApi.update(id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: titleQueryKey(id) });
      void queryClient.invalidateQueries({ queryKey: CATALOG_QUERY_KEY });
    },
  });
}

export function useUploadCoverMutation(id: number) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (file: File) => catalogApi.uploadCover(id, file),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: titleQueryKey(id) });
      void queryClient.invalidateQueries({ queryKey: CATALOG_QUERY_KEY });
    },
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
