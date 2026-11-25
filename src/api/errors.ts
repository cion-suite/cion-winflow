import { apiErrorKey } from "@/lib/apiErrors";
import { useT } from "@/lib/i18n";
import { ApiError } from "./client";

/** Текст ошибки API для формы: известный code переводится, иначе — message. */
export function useErrorText(error: unknown): string | null {
  const t = useT();
  if (!error) return null;

  if (error instanceof ApiError) {
    const key = apiErrorKey(error.code);
    return key ? t(key) : error.message;
  }

  return error instanceof Error ? error.message : t("errors.unknown");
}
