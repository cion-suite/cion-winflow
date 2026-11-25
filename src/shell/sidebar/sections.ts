import type { ComponentType, SVGProps } from "react";
import {
  BookMarkedIcon,
  LibraryBigIcon,
  ScrollTextIcon,
  SettingsIcon,
  ShieldIcon,
} from "lucide-react";

export type SectionGroup = "overview" | "footer";

export type Section = {
  path: string;
  /** Ключ i18n заголовка: он же подпись в сайдбаре и в шапке. */
  i18nKey: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  group: SectionGroup;
  /** Пункт виден только администратору; сервер всё равно проверяет роль. */
  adminOnly?: boolean;
};

/** Группы с заголовком. Футер (`footer`) остаётся без заголовка. */
export const SIDEBAR_GROUPS = [
  { id: "overview", i18nKey: "nav.groups.overview" },
] as const satisfies readonly { id: SectionGroup; i18nKey: string }[];

const ROWS = [
  { path: "/", i18nKey: "nav.catalog", icon: LibraryBigIcon, group: "overview" },
  { path: "/library", i18nKey: "nav.library", icon: BookMarkedIcon, group: "overview" },
  {
    path: "/admin",
    i18nKey: "nav.admin",
    icon: ShieldIcon,
    group: "overview",
    adminOnly: true,
  },
  { path: "/changelog", i18nKey: "nav.changelog", icon: ScrollTextIcon, group: "footer" },
  { path: "/settings", i18nKey: "nav.settings", icon: SettingsIcon, group: "footer" },
] as const satisfies readonly Section[];

export type SectionPath = (typeof ROWS)[number]["path"];

/** Единая таблица: сайдбар, шапка и роутер читают её. */
export const SECTIONS: readonly Section[] = ROWS;

/** Раздел по пути. Завершающий слэш не делает путь другим: /settings/ - тот же экран. */
export function sectionForPath(pathname: string): Section | undefined {
  const normalized = pathname.length > 1 ? pathname.replace(/\/+$/, "") : pathname;
  return SECTIONS.find((section) =>
    section.path === "/"
      ? normalized === "/"
      : normalized === section.path || normalized.startsWith(`${section.path}/`),
  );
}

/**
 * Экраны вне меню: параметрические маршруты не входят в SectionPath,
 * но шапке нужен их заголовок.
 */
const EXTRA_PAGE_PREFIXES = [
  { prefix: "/title/", i18nKey: "nav.title" },
  { prefix: "/reader/", i18nKey: "nav.reader" },
  { prefix: "/watch/", i18nKey: "nav.watch" },
] as const;

export function pageTitleKey(pathname: string): string {
  const section = sectionForPath(pathname);
  if (section) return section.i18nKey;

  const extra = EXTRA_PAGE_PREFIXES.find(({ prefix }) => pathname.startsWith(prefix));
  return extra?.i18nKey ?? "nav.unknown";
}
