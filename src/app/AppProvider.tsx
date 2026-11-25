import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router";

import { AuthProvider } from "@/api";
import { ApiError } from "@/api/client";
import { TooltipProvider } from "@/ui/shadcn/tooltip";

function shouldRetry(failureCount: number, error: Error): boolean {
  // 401 не ретраим: это гость или истёкшая сессия, AuthProvider разберётся
  if (error instanceof ApiError && error.status === 401) return false;
  return failureCount < 2;
}

// HTTP-клиент: повторы и обновление при фокусе - обычные умолчания TanStack Query.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: shouldRetry },
    mutations: { retry: false },
  },
});

type AppProviderProps = {
  children: ReactNode;
};

export function AppProvider({ children }: AppProviderProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AuthProvider>
          <TooltipProvider>{children}</TooltipProvider>
        </AuthProvider>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
