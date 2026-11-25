import { LogOutIcon } from "lucide-react";
import { useLocation, useNavigate } from "react-router";

import { useAuth, useLogoutMutation } from "@/api";
import { useT } from "@/lib/i18n";
import { pageTitleKey } from "@/shell/sidebar/sections";
import { Button } from "@/ui/shadcn/button";
import { LocaleMenu } from "./LocaleMenu";
import { SidebarToggle } from "./SidebarToggle";
import { ThemeToggle } from "./ThemeToggle";

function PageTitle() {
  const t = useT();
  const { pathname } = useLocation();

  return (
    <span className="truncate text-sm font-medium">{t(pageTitleKey(pathname))}</span>
  );
}

function UserMenu() {
  const t = useT();
  const { user } = useAuth();
  const logout = useLogoutMutation();
  const navigate = useNavigate();

  // RequireAuth не рендерит Layout без user; guard сужает тип из useAuth
  if (!user) return null;

  return (
    <div className="flex items-center gap-1">
      <span className="hidden max-w-32 truncate text-sm text-muted-foreground sm:inline">
        {user.username}
      </span>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        aria-label={t("auth.logout")}
        title={t("auth.logout")}
        disabled={logout.isPending}
        onClick={() =>
          logout.mutate(undefined, {
            onSuccess: () => void navigate("/login", { replace: true }),
          })
        }
      >
        <LogOutIcon />
      </Button>
    </div>
  );
}

export function Navbar() {
  return (
    <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
      <SidebarToggle />
      <PageTitle />
      <div className="ml-auto flex items-center gap-1">
        <UserMenu />
        <LocaleMenu />
        <ThemeToggle />
      </div>
    </header>
  );
}
