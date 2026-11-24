import { useLocation } from "react-router";

import { useT } from "@/lib/i18n";
import { pageTitleKey } from "@/shell/sidebar/sections";
import { LocaleMenu } from "./LocaleMenu";
import { SidebarToggle } from "./SidebarToggle";
import { ThemeToggle } from "./ThemeToggle";

function PageTitle() {
  const t = useT();
  const { pathname } = useLocation();

  return (
    <span className="truncate text-sm font-medium">{t(pageTitleKey(pathname))}</span>
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
