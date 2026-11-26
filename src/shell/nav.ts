import {
  SettingsIcon,
  ShieldIcon,
  SparklesIcon,
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
  { path: "/changelog", i18nKey: "nav.changelog", icon: SparklesIcon },
  { path: "/settings", i18nKey: "nav.settings", icon: SettingsIcon },
  { path: "/admin", i18nKey: "nav.admin", icon: ShieldIcon, adminOnly: true },
];
