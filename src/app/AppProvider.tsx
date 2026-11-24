import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router";

import { TooltipProvider } from "@/ui/shadcn/tooltip";

// HTTP-клиент: повторы и обновление при фокусе - обычные умолчания TanStack Query.
// 401 будет разбираться в AuthProvider, когда появится авторизация (фаза 2).
const queryClient = new QueryClient();

type AppProviderProps = {
  children: ReactNode;
};

export function AppProvider({ children }: AppProviderProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <TooltipProvider>{children}</TooltipProvider>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
