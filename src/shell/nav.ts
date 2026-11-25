export type NavItem = {
  path: string;
  /** Ключ i18n подписи. */
  i18nKey: string;
  adminOnly?: boolean;
};

/** Ссылки в шапке: библиотека — дом приложения, остальное — в меню пользователя. */
export const MAIN_NAV: readonly NavItem[] = [
  { path: "/", i18nKey: "nav.library" },
  { path: "/changelog", i18nKey: "nav.changelog" },
];

/** Меню пользователя: на телефоне ссылки шапки скрыты, поэтому changelog здесь тоже. */
export const MENU_NAV: readonly NavItem[] = [
  { path: "/changelog", i18nKey: "nav.changelog" },
  { path: "/settings", i18nKey: "nav.settings" },
  { path: "/admin", i18nKey: "nav.admin", adminOnly: true },
];

/** Активный пункт: "/" не подсвечивается на вложенных экранах. */
export function isActivePath(pathname: string, path: string): boolean {
  if (path === "/") return pathname === "/";
  return pathname === path || pathname.startsWith(`${path}/`);
}
