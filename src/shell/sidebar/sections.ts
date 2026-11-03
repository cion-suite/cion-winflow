import type { ComponentType, SVGProps } from "react";
import { HomeIcon, NetworkIcon, ScrollTextIcon, SettingsIcon } from "lucide-react";

export type SectionGroup = "main" | "footer";

export type Section = {
  path: string;
  /** Ключ i18n заголовка: он же подпись в сайдбаре и в шапке. */
  i18nKey: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  group: SectionGroup;
};

const ROWS = [
  { path: "/", i18nKey: "nav.home", icon: HomeIcon, group: "main" },
  { path: "/network", i18nKey: "nav.network", icon: NetworkIcon, group: "main" },
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
