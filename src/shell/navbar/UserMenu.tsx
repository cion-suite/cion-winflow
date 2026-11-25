import { LogOutIcon, ShieldIcon, UserIcon } from "lucide-react";
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

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="ghost" size="sm" aria-label={t("nav.account")} />}
      >
        <UserIcon data-icon="inline-start" />
        <span className="hidden max-w-32 truncate sm:inline">{user.username}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{user.username}</DropdownMenuLabel>
          {items.map((item) => (
            <DropdownMenuItem
              key={item.path}
              onClick={() => void navigate(item.path)}
            >
              {item.path === "/admin" && <ShieldIcon />}
              {t(item.i18nKey)}
            </DropdownMenuItem>
          ))}
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem
            variant="destructive"
            disabled={logout.isPending}
            onClick={() =>
              logout.mutate(undefined, {
                onSuccess: () => void navigate("/login", { replace: true }),
              })
            }
          >
            <LogOutIcon />
            {t("auth.logout")}
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
