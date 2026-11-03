import { ThemeToggle } from "./ThemeToggle";

export function Navbar() {
  return (
    <header className="flex h-14 shrink-0 items-center justify-end border-b px-4">
      <ThemeToggle />
    </header>
  );
}
