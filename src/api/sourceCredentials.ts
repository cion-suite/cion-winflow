import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { jsonMutation, request } from "./client";

/**
 * Состояние личной учётки источника. Секретов в ответе нет и быть не должно:
 * сервер отдаёт только признак подключения и метаданные проверки.
 */
export type SourceCredential = {
  code: string;
  name: string;
  configured: boolean;
  updated_at: number | null;
  last_verified_at: number | null;
  /** Код последней ошибки авторизации, без секретов. */
  last_error: string | null;
};

export type SourceCredentialsResponse = {
  credentials: SourceCredential[];
};

export type SourceCredentialInput = {
  bearer?: string | null;
  cookie?: string | null;
};

export const SOURCE_CREDENTIALS_QUERY_KEY = ["source-credentials"] as const;

function credentialPath(code: string): string {
  return `/me/source-credentials/${encodeURIComponent(code)}`;
}

export const sourceCredentialsApi = {
  list: () => request<SourceCredentialsResponse>("/me/source-credentials"),
  save: (code: string, input: SourceCredentialInput) =>
    request<SourceCredential>(credentialPath(code), jsonMutation(input, "PUT")),
  remove: (code: string) =>
    request<void>(credentialPath(code), { method: "DELETE" }),
};

export function useSourceCredentialsQuery() {
  return useQuery({
    queryKey: SOURCE_CREDENTIALS_QUERY_KEY,
    queryFn: sourceCredentialsApi.list,
  });
}

/**
 * Учётки per-user: после сохранения или удаления меняется доступ к главам,
 * поэтому инвалидируем не только список учёток, но и тайтл, библиотеку и
 * контент главы. TTL sync на сервере уже сброшен, следующий запрос увидит
 * изменения.
 */
function invalidateAfterCredentialChange(
  queryClient: ReturnType<typeof useQueryClient>,
): void {
  void queryClient.invalidateQueries({ queryKey: SOURCE_CREDENTIALS_QUERY_KEY });
  void queryClient.invalidateQueries({ queryKey: ["title"] });
  void queryClient.invalidateQueries({ queryKey: ["library"] });
  void queryClient.invalidateQueries({ queryKey: ["chapter-content"] });
}

export function useSaveSourceCredentialMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      code,
      input,
    }: {
      code: string;
      input: SourceCredentialInput;
    }) => sourceCredentialsApi.save(code, input),
    onSuccess: () => invalidateAfterCredentialChange(queryClient),
  });
}

export function useDeleteSourceCredentialMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (code: string) => sourceCredentialsApi.remove(code),
    onSuccess: () => invalidateAfterCredentialChange(queryClient),
  });
}
