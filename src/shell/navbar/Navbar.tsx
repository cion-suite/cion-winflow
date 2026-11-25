import { Link, useLocation } from "react-router";

import { useAuth } from "@/api";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { isActivePath, MAIN_NAV } from "@/shell/nav";
import { buttonVariants } from "@/ui/shadcn/button";
import { LibrarySearch } from "./LibrarySearch";
import { LocaleMenu } from "./LocaleMenu";
import { ThemeToggle } from "./ThemeToggle";
import { UserMenu } from "./UserMenu";

export function Navbar() {
  const t = useT();
  const { pathname } = useLocation();
  const { user } = useAuth();
  const items = MAIN_NAV.filter(
    (item) => !item.adminOnly || user?.role === "admin",
  );

  return (
    <header className="sticky top-0 z-30 flex h-14 shrink-0 items-center gap-3 border-b bg-background/95 px-4 backdrop-blur">
      <Link
        to="/"
        className="font-heading text-sm font-semibold tracking-tight whitespace-nowrap"
      >
        cion-reader
      </Link>

      <nav className="hidden items-center gap-1 md:flex">
        {items.map((item) => (
          <Link
            key={item.path}
            to={item.path}
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              isActivePath(pathname, item.path) && "bg-muted text-foreground",
            )}
          >
            {t(item.i18nKey)}
          </Link>
        ))}
      </nav>

      <div className="flex min-w-0 flex-1 justify-center px-2">
        <div className="w-full max-w-md">
          <LibrarySearch />
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        <LocaleMenu />
        <ThemeToggle />
        <UserMenu />
      </div>
    </header>
  );
}
