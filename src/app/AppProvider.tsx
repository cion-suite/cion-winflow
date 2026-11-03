import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { HashRouter } from "react-router";

import { TooltipProvider } from "@/ui/shadcn/tooltip";

// Commands run locally: "online" mode would pause them while the OS reports
// no network, and retries or refetch on focus would only repeat the same call.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      networkMode: "always",
      refetchOnWindowFocus: false,
      retry: false,
    },
    mutations: { networkMode: "always" },
  },
});

type AppProviderProps = {
  children: ReactNode;
};

export function AppProvider({ children }: AppProviderProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <TooltipProvider>{children}</TooltipProvider>
      </HashRouter>
    </QueryClientProvider>
  );
}
