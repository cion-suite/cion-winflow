import { useNavigate } from "react-router";

import { useAuth, useLogoutMutation, userAvatarUrl } from "@/api";
import { useT } from "@/lib/i18n";
import { menuItemsForRole, roleLabelKey } from "@/lib/nav";
import { AccountMenu } from "@/ui/AccountMenu";

/**
 * То же меню, что в шапке, но страницы не могут импортировать shell:
 * связанная обёртка живёт здесь, а разметка — в ui/AccountMenu.
 */
export function ReaderUserMenu() {
  const t = useT();
  const navigate = useNavigate();
  const { user } = useAuth();
  const logout = useLogoutMutation();

  if (!user) return null;

  return (
    <AccountMenu
      username={user.username}
      roleLabel={t(roleLabelKey(user.role))}
      items={menuItemsForRole(user.role)}
      variant="avatar"
      avatarSrc={userAvatarUrl(
        user.id,
        user.avatar_updated_at,
        user.avatar_version,
      )}
      logoutPending={logout.isPending}
      onNavigate={(path) => void navigate(path)}
      onLogout={() =>
        logout.mutate(undefined, {
          onSuccess: () => void navigate("/login", { replace: true }),
        })
      }
    />
  );
}
