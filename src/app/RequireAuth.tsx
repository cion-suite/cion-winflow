import { Navigate, Outlet, useLocation } from "react-router";

import { useAuth } from "@/api";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";

/** Защищает экраны приложения: гость уходит на /login с запомненным адресом. */
export function RequireAuth() {
  const { user, isLoading, error, retry } = useAuth();
  const location = useLocation();
  const t = useT();

  if (isLoading) {
    return (
      <div className="grid h-svh place-items-center text-sm text-muted-foreground">
        {t("auth.loading")}
      </div>
    );
  }

  if (error) {
    return (
      <div className="grid h-svh place-items-center p-6">
        <div className="flex max-w-sm flex-col items-center gap-2 text-center">
          <p className="text-sm text-destructive">{t("auth.errors.unknown")}</p>
          <p className="text-xs text-muted-foreground">{error.message}</p>
          <Button type="button" variant="outline" onClick={retry}>
            {t("auth.retry")}
          </Button>
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <Navigate
        to="/login"
        replace
        state={{ from: `${location.pathname}${location.search}` }}
      />
    );
  }

  return <Outlet />;
}
