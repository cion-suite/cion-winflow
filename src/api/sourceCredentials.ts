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
  /** Источник умеет серверный вход по логину/паролю. */
  login_supported: boolean;
  updated_at: number | null;
  last_verified_at: number | null;
  /** Код последней ошибки авторизации, без секретов. */
  last_error: string | null;
};

export type SourceCredentialsResponse = {
  credentials: SourceCredential[];
};

/** Пара для серверного входа. Пароль уходит один раз и не сохраняется. */
export type SourceLoginInput = {
  login: string;
  password: string;
};

export const SOURCE_CREDENTIALS_QUERY_KEY = ["source-credentials"] as const;

function credentialPath(code: string): string {
  return `/me/source-credentials/${encodeURIComponent(code)}`;
}

export const sourceCredentialsApi = {
  list: () => request<SourceCredentialsResponse>("/me/source-credentials"),
  login: (code: string, input: SourceLoginInput) =>
    request<SourceCredential>(
      `${credentialPath(code)}/login`,
      jsonMutation(input, "POST"),
    ),
  verify: (code: string) =>
    request<SourceCredential>(
      `${credentialPath(code)}/verify`,
      jsonMutation({}, "POST"),
    ),
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

export function useLoginSourceCredentialMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      code,
      input,
    }: {
      code: string;
      input: SourceLoginInput;
    }) => sourceCredentialsApi.login(code, input),
    onSuccess: () => invalidateAfterCredentialChange(queryClient),
  });
}

export function useVerifySourceCredentialMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (code: string) => sourceCredentialsApi.verify(code),
    // Проверка меняет last_error и на 502: инвалидируем и при ошибке, иначе
    // карточка останется зелёной после отклонённого токена.
    onSettled: () => invalidateAfterCredentialChange(queryClient),
  });
}

export function useDeleteSourceCredentialMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (code: string) => sourceCredentialsApi.remove(code),
    onSuccess: () => invalidateAfterCredentialChange(queryClient),
  });
}
