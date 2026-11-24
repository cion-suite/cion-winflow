// Тема до первой отрисовки: внешний файл вместо inline-скрипта, иначе CSP
// потребует 'unsafe-inline' или хеша. Тот же ключ читает ThemeToggle.
let saved = null;
try {
  saved = localStorage.getItem("theme");
} catch {
  // localStorage может быть недоступен: остаётся тема системы
}

const theme =
  saved ?? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.documentElement.classList.toggle("dark", theme === "dark");
