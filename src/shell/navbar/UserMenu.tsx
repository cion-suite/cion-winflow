import { LogOutIcon, UserIcon } from "lucide-react";
import { useNavigate } from "react-router";

import { useAuth, useLogoutMutation } from "@/api";
import { useT } from "@/lib/i18n";
import { MENU_NAV } from "@/shell/nav";
import { Button } from "@/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui/shadcn/dropdown-menu";

export function UserMenu() {
  const t = useT();
  const navigate = useNavigate();
  const { user } = useAuth();
  const logout = useLogoutMutation();

  // RequireAuth не рендерит Layout без user
  if (!user) return null;

  const items = MENU_NAV.filter(
    (item) => !item.adminOnly || user.role === "admin",
  );
  const role = user.role === "admin" ? "auth.roles.admin" : "auth.roles.user";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="ghost" size="sm" aria-label={t("nav.account")} />}
      >
        <UserIcon data-icon="inline-start" />
        <span className="hidden max-w-32 truncate sm:inline">{user.username}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-60 p-1.5">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="px-2 py-1.5">
            <div className="flex flex-col gap-1">
              <span className="truncate text-sm leading-none font-medium text-foreground">
                {user.username}
              </span>
              <span className="text-xs leading-none text-muted-foreground">
                {t(role)}
              </span>
            </div>
          </DropdownMenuLabel>
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <DropdownMenuItem
                key={item.path}
                className="gap-2 px-2 py-1.5"
                onClick={() => void navigate(item.path)}
              >
                <Icon />
                <span>{t(item.i18nKey)}</span>
              </DropdownMenuItem>
            );
          })}
        </DropdownMenuGroup>
        <DropdownMenuSeparator className="my-1.5" />
        <DropdownMenuItem
          variant="destructive"
          className="gap-2 px-2 py-1.5"
          disabled={logout.isPending}
          onClick={() =>
            logout.mutate(undefined, {
              onSuccess: () => void navigate("/login", { replace: true }),
            })
          }
        >
          <LogOutIcon />
          <span>{t("auth.logout")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
