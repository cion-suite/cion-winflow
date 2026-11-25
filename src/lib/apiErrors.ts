/** Код ошибки API -> ключ i18n; неизвестный код показываем текстом сервера. */
const ERROR_KEYS: Partial<Record<string, string>> = {
  invalid_credentials: "auth.errors.invalidCredentials",
  invalid_invite: "auth.errors.invalidInvite",
  registration_closed: "auth.errors.registrationClosed",
  username_taken: "auth.errors.usernameTaken",
  origin_not_allowed: "auth.errors.originNotAllowed",
  rate_limited: "auth.errors.rateLimited",
  validation: "errors.validation",
  title_not_found: "errors.titleNotFound",
  kind_locked: "errors.kindLocked",
  cover_file_required: "errors.coverFileRequired",
  cover_too_large: "errors.coverTooLarge",
  cover_invalid: "errors.coverInvalid",
  library_status_invalid: "errors.libraryStatusInvalid",
  score_invalid: "errors.scoreInvalid",
  not_found: "errors.notFound",
  network_error: "errors.network",
  internal_error: "errors.unknown",
};

export function apiErrorKey(code: string | null): string | null {
  return code ? (ERROR_KEYS[code] ?? null) : null;
}
