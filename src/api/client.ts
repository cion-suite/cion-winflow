const API_BASE = "/api";

export type ApiErrorPayload = {
  code?: unknown;
  message?: unknown;
};

/** Ошибка API: HTTP-статус и машинный code сохраняются для обработки на месте. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string | null;

  constructor(status: number, code: string | null, message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

/** Адрес API для мест, где нужен URL, а не fetch: <img src>, ссылки на скачивание. */
export function apiUrl(path: string): string {
  return `${API_BASE}${path}`;
}

/** RequestInit для JSON-мутации: одинаковый заголовок во всех областях API. */
export function jsonMutation(body: unknown, method: "POST" | "PUT" = "POST"): RequestInit {
  return {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  };
}

async function errorFrom(response: Response): Promise<ApiError> {
  let payload: ApiErrorPayload | null = null;
  try {
    payload = (await response.json()) as ApiErrorPayload;
  } catch {
    // Не JSON: сообщение соберём по статусу, тело ответа в текст ошибки не тащим
  }

  const message =
    typeof payload?.message === "string" && payload.message
      ? payload.message
      : `Request failed with status ${response.status}`;
  const code = typeof payload?.code === "string" ? payload.code : null;
  return new ApiError(response.status, code, message);
}

/**
 * Запрос к API. Путь задаётся относительно /api: request("/health") -> /api/health.
 * Ошибка сервера приходит как { code, message } и становится ApiError;
 * сетевой сбой - ApiError со status 0.
 */
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has("Accept")) headers.set("Accept", "application/json");

  let response: Response;
  try {
    response = await fetch(apiUrl(path), {
      credentials: "same-origin",
      ...init,
      headers,
    });
  } catch (cause) {
    throw new ApiError(0, "network_error", "Network request failed", { cause });
  }

  if (!response.ok) throw await errorFrom(response);
  if (response.status === 204) return undefined as T;

  const text = await response.text();
  if (text === "") return undefined as T;

  try {
    return JSON.parse(text) as T;
  } catch (cause) {
    throw new ApiError(
      response.status,
      "invalid_response",
      "Server returned invalid JSON",
      { cause },
    );
  }
}
