import {
  SettingsIcon,
  ShieldIcon,
  SparklesIcon,
  UserRoundIcon,
  type LucideIcon,
} from "lucide-react";

export type NavItem = {
  path: string;
  /** Ключ i18n подписи. */
  i18nKey: string;
  icon: LucideIcon;
  adminOnly?: boolean;
};

/** Меню пользователя: разделы, которых нет в шапке. */
export const MENU_NAV: readonly NavItem[] = [
  { path: "/profile", i18nKey: "nav.profile", icon: UserRoundIcon },
  { path: "/changelog", i18nKey: "nav.changelog", icon: SparklesIcon },
  { path: "/settings", i18nKey: "nav.settings", icon: SettingsIcon },
  { path: "/admin", i18nKey: "nav.admin", icon: ShieldIcon, adminOnly: true },
];

/** Пункты меню доступные роли: админка только администратору. */
export function menuItemsForRole(role: string): readonly NavItem[] {
  return MENU_NAV.filter((item) => !item.adminOnly || role === "admin");
}

export function roleLabelKey(role: string): string {
  return role === "admin" ? "auth.roles.admin" : "auth.roles.user";
}
