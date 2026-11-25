const AUTH_ERROR_KEYS: Partial<Record<string, string>> = {
  invalid_credentials: "auth.errors.invalidCredentials",
  invalid_invite: "auth.errors.invalidInvite",
  registration_closed: "auth.errors.registrationClosed",
  username_taken: "auth.errors.usernameTaken",
  origin_not_allowed: "auth.errors.originNotAllowed",
  rate_limited: "auth.errors.rateLimited",
  validation: "auth.errors.validation",
  network_error: "auth.errors.network",
};

/** Код ошибки API -> ключ i18n; неизвестный код показываем текстом сервера. */
export function authErrorKey(code: string | null): string | null {
  return code ? (AUTH_ERROR_KEYS[code] ?? null) : null;
}
