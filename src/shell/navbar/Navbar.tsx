import { Link } from "react-router";

import { LibrarySearch } from "./LibrarySearch";
import { LocaleMenu } from "./LocaleMenu";
import { ThemeToggle } from "./ThemeToggle";
import { UserMenu } from "./UserMenu";

export function Navbar() {
  return (
    <header className="sticky top-0 z-30 flex h-14 shrink-0 items-center gap-3 border-b bg-background/95 px-4 backdrop-blur">
      <Link
        to="/"
        className="flex items-center gap-1 font-heading text-sm font-semibold tracking-tight whitespace-nowrap"
      >
        <img
          src="/logo.png"
          alt=""
          width={28}
          height={28}
          className="size-5 shrink-0"
        />
        cion-reader
      </Link>

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
