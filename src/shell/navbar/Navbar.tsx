import { useLocation } from "react-router";

import { useT } from "@/lib/i18n";
import { sectionForPath } from "@/shell/sidebar/sections";
import { LocaleMenu } from "./LocaleMenu";
import { SidebarToggle } from "./SidebarToggle";
import { ThemeToggle } from "./ThemeToggle";

function PageTitle() {
  const t = useT();
  const { pathname } = useLocation();
  const current = sectionForPath(pathname);

  return (
    <span className="truncate text-sm font-medium">
      {t(current?.i18nKey ?? "nav.unknown")}
    </span>
  );
}

export function Navbar() {
  return (
    <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
      <SidebarToggle />
      <PageTitle />
      <div className="ml-auto flex items-center gap-1">
        <LocaleMenu />
        <ThemeToggle />
      </div>
    </header>
  );
}
