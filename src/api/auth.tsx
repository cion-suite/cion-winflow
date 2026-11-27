import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError, jsonMutation, request } from "./client";

export type Role = "admin" | "user";

export type User = {
  id: number;
  username: string;
  role: Role;
  /** NULL — аватара нет; иначе время загрузки. */
  avatar_updated_at: number | null;
  /** Монотонная версия аватара для ETag и cache-buster. */
  avatar_version: number;
};

export type MeResponse = {
  user: User;
};

type Credentials = {
  username: string;
  password: string;
};

type RegisterInput = Credentials & {
  invite: string;
};

export type ChangePasswordInput = {
  current_password: string;
  new_password: string;
};

export const ME_QUERY_KEY = ["me"] as const;

function postJson<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, jsonMutation(body));
}

/** 401 — это «гость», а не ошибка: экран решает RequireAuth. */
async function fetchMe(): Promise<MeResponse | null> {
  try {
    return await request<MeResponse>("/me");
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) return null;
    throw error;
  }
}

export const authApi = {
  me: fetchMe,
  login: (credentials: Credentials) => postJson<MeResponse>("/auth/login", credentials),
  register: (input: RegisterInput) => postJson<MeResponse>("/auth/register", input),
  logout: () => request<void>("/auth/logout", { method: "POST" }),
  updateUsername: (username: string) =>
    request<MeResponse>("/me/username", jsonMutation({ username }, "PUT")),
  changePassword: (input: ChangePasswordInput) =>
    request<void>("/me/password", jsonMutation(input, "PUT")),
};

export function useMeQuery() {
  return useQuery({
    queryKey: ME_QUERY_KEY,
    queryFn: fetchMe,
    retry: false,
    staleTime: 60_000,
  });
}

export function useLoginMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: authApi.login,
    onSuccess: (data) => {
      queryClient.setQueryData(ME_QUERY_KEY, data);
    },
  });
}

export function useRegisterMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: authApi.register,
    onSuccess: (data) => {
      queryClient.setQueryData(ME_QUERY_KEY, data);
    },
  });
}

export function useLogoutMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: authApi.logout,
    // Кэш прошлого пользователя не должен остаться на общем браузере
    onSuccess: () => queryClient.clear(),
  });
}

export function useUpdateUsernameMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: authApi.updateUsername,
    onSuccess: (data) => {
      queryClient.setQueryData(ME_QUERY_KEY, data);
      // Ник виден и в публичном профиле.
      void queryClient.invalidateQueries({ queryKey: ["profile"] });
    },
  });
}

export function useChangePasswordMutation() {
  return useMutation({
    mutationFn: authApi.changePassword,
  });
}

type AuthState = {
  user: User | null;
  isLoading: boolean;
  error: Error | null;
  retry: () => void;
};

const AuthContext = createContext<AuthState | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const me = useMeQuery();
  const handledUnauthorized = useRef(false);

  useEffect(() => {
    // 401 от любого запроса — сессия истекла: чистим кэш и показываем login.
    // Login/register — мутации, поэтому неверный пароль сюда не попадает.
    return queryClient.getQueryCache().subscribe((event) => {
      if (event.type !== "updated") return;

      const error = event.query.state.error;
      if (!(error instanceof ApiError) || error.status !== 401) return;
      if (handledUnauthorized.current) return;

      handledUnauthorized.current = true;
      queryClient.clear();
    });
  }, [queryClient]);

  const user = me.data?.user ?? null;
  const { isPending, error, refetch } = me;

  useEffect(() => {
    if (user) handledUnauthorized.current = false;
  }, [user]);

  const value = useMemo<AuthState>(
    () => ({
      user,
      isLoading: isPending,
      error,
      retry: () => void refetch(),
    }),
    [user, isPending, error, refetch],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthState {
  const value = useContext(AuthContext);
  if (!value) throw new Error("useAuth must be used within AuthProvider");
  return value;
}
