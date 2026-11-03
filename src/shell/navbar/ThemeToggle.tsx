import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/ui/shadcn/button";

// index.html reads the same key to set the theme before the first paint
const THEME_KEY = "theme";

// The native title bar is not part of the page, Tauri repaints it separately
function syncWindowTheme(isDark: boolean) {
  // getCurrentWindow throws in a plain browser, for example with `pnpm dev`
  if (!isTauri()) return;

  getCurrentWindow()
    .setTheme(isDark ? "dark" : "light")
    .catch(console.error);
}

function toggleTheme() {
  const isDark = document.documentElement.classList.toggle("dark");
  localStorage.setItem(THEME_KEY, isDark ? "dark" : "light");
  syncWindowTheme(isDark);
}

function ThemeIcon() {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M12 12m-9 0a9 9 0 1 0 18 0a9 9 0 1 0 -18 0" />
      <path d="M12 3l0 18" />
      <path d="M12 9l4.65 -4.65" />
      <path d="M12 14.3l7.37 -7.37" />
      <path d="M12 19.6l8.85 -8.85" />
    </svg>
  );
}

export function ThemeToggle() {
  useEffect(() => {
    syncWindowTheme(document.documentElement.classList.contains("dark"));
  }, []);

  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Toggle theme"
      title="Toggle theme"
      onClick={toggleTheme}
    >
      <ThemeIcon />
    </Button>
  );
}
