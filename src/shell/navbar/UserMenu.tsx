import { useNavigate } from "react-router";

import { useAuth, useLogoutMutation } from "@/api";
import { useT } from "@/lib/i18n";
import { menuItemsForRole, roleLabelKey } from "@/lib/nav";
import { AccountMenu } from "@/ui/AccountMenu";

export function UserMenu() {
  const t = useT();
  const navigate = useNavigate();
  const { user } = useAuth();
  const logout = useLogoutMutation();

  // RequireAuth не рендерит Layout без user
  if (!user) return null;

  return (
    <AccountMenu
      username={user.username}
      roleLabel={t(roleLabelKey(user.role))}
      items={menuItemsForRole(user.role)}
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
