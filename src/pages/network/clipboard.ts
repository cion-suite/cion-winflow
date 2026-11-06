/**
 * Копирование в буфер обмена. Clipboard API требует защищённого контекста,
 * поэтому для окружений без него остаётся скрытая textarea и execCommand.
 */
export async function copyText(text: string): Promise<void> {
  if (navigator.clipboard && window.isSecureContext) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // API может отклонить запрос: например, окно не в фокусе. Ниже запасной путь.
    }
  }

  const previousFocus =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.tabIndex = -1;
  textarea.setAttribute("readonly", "");
  textarea.setAttribute("aria-hidden", "true");
  textarea.style.position = "fixed";
  textarea.style.top = "0";
  textarea.style.left = "-9999px";
  document.body.append(textarea);

  try {
    textarea.select();
    if (!document.execCommand("copy")) throw new Error("Clipboard is unavailable");
  } finally {
    textarea.remove();
    previousFocus?.focus();
  }
}
