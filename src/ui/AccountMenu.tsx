import { LogOutIcon, UserIcon } from "lucide-react";

import { useT } from "@/lib/i18n";
import type { NavItem } from "@/lib/nav";
import { Avatar, AvatarFallback } from "@/ui/shadcn/avatar";
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

type AccountMenuProps = {
  username: string;
  roleLabel: string;
  items: readonly NavItem[];
  logoutPending: boolean;
  onNavigate: (path: string) => void;
  onLogout: () => void;
  /** `avatar` — круглая кнопка с инициалом вместо иконки и имени. */
  variant?: "default" | "avatar";
};

/** Меню пользователя без знания API: данные и действия приходят сверху. */
export function AccountMenu({
  username,
  roleLabel,
  items,
  logoutPending,
  onNavigate,
  onLogout,
  variant = "default",
}: AccountMenuProps) {
  const t = useT();
  const avatar = variant === "avatar";
  const initial = username.trim().slice(0, 1).toUpperCase();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size={avatar ? "icon-lg" : "sm"}
            className={avatar ? "rounded-full p-0" : undefined}
            aria-label={t("nav.account")}
          />
        }
      >
        {avatar ? (
          <Avatar>
            <AvatarFallback>{initial}</AvatarFallback>
          </Avatar>
        ) : (
          <>
            <UserIcon data-icon="inline-start" />
            <span className="hidden max-w-32 truncate sm:inline">
              {username}
            </span>
          </>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-60 p-1.5">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="px-2 py-1.5">
            <div className="flex flex-col gap-1">
              <span className="truncate text-sm leading-none font-medium text-foreground">
                {username}
              </span>
              <span className="text-xs leading-none text-muted-foreground">
                {roleLabel}
              </span>
            </div>
          </DropdownMenuLabel>
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <DropdownMenuItem
                key={item.path}
                className="gap-2 px-2 py-1.5"
                onClick={() => onNavigate(item.path)}
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
          disabled={logoutPending}
          onClick={onLogout}
        >
          <LogOutIcon />
          <span>{t("auth.logout")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
