import type { ComponentType, SVGProps } from "react";
import {
  AppWindowIcon,
  HomeIcon,
  NetworkIcon,
  ScrollTextIcon,
  SettingsIcon,
  SlidersHorizontalIcon,
  WrenchIcon,
} from "lucide-react";

export type SectionGroup = "overview" | "workshop" | "footer";

export type Section = {
  path: string;
  /** Ключ i18n заголовка: он же подпись в сайдбаре и в шапке. */
  i18nKey: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  group: SectionGroup;
};

/** Группы с заголовком. Футер (`footer`) остаётся без заголовка. */
export const SIDEBAR_GROUPS = [
  { id: "overview", i18nKey: "nav.groups.overview" },
  { id: "workshop", i18nKey: "nav.groups.workshop" },
] as const satisfies readonly { id: SectionGroup; i18nKey: string }[];

const ROWS = [
  { path: "/", i18nKey: "nav.home", icon: HomeIcon, group: "overview" },
  { path: "/network", i18nKey: "nav.network", icon: NetworkIcon, group: "overview" },
  { path: "/apps", i18nKey: "nav.apps", icon: AppWindowIcon, group: "workshop" },
  { path: "/utils", i18nKey: "nav.utils", icon: WrenchIcon, group: "workshop" },
  { path: "/system", i18nKey: "nav.system", icon: SlidersHorizontalIcon, group: "workshop" },
  { path: "/changelog", i18nKey: "nav.changelog", icon: ScrollTextIcon, group: "footer" },
  { path: "/settings", i18nKey: "nav.settings", icon: SettingsIcon, group: "footer" },
] as const satisfies readonly Section[];

export type SectionPath = (typeof ROWS)[number]["path"];

/** Единая таблица: сайдбар, шапка и роутер читают её. */
export const SECTIONS: readonly Section[] = ROWS;

/** Раздел по пути. Завершающий слэш не делает путь другим: /settings/ - тот же экран. */
export function sectionForPath(pathname: string): Section | undefined {
  const normalized = pathname.length > 1 ? pathname.replace(/\/+$/, "") : pathname;
  return SECTIONS.find((section) => section.path === normalized);
}
