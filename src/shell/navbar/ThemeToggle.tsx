import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";

// public/theme.js reads the same key to set the theme before the first paint
const THEME_KEY = "theme";

function toggleTheme() {
  const isDark = document.documentElement.classList.toggle("dark");
  try {
    localStorage.setItem(THEME_KEY, isDark ? "dark" : "light");
  } catch (error) {
    // Хранилище может быть недоступно; тема всё равно переключилась в DOM
    console.error("Failed to save theme", error);
  }
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
  const t = useT();
  const label = t("theme.toggle");

  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      title={label}
      onClick={toggleTheme}
    >
      <ThemeIcon />
    </Button>
  );
}
