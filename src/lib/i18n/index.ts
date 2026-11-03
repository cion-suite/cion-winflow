import i18n from "i18next";
import { initReactI18next, useTranslation } from "react-i18next";

import { settings, useSetting } from "@/lib/settings";
import { DEFAULT_LOCALE, resources, SUPPORTED_LOCALES, type SupportedLocale } from "./resources";

export { DEFAULT_LOCALE, SUPPORTED_LOCALES, type SupportedLocale };
export { i18n };

/** Язык задаёт settings "locale", i18next лишь следует за ним. */
export function initI18n(): void {
  const locale = settings.get("locale");
  void i18n.use(initReactI18next).init({
    resources,
    lng: locale,
    fallbackLng: DEFAULT_LOCALE,
    defaultNS: "common",
    ns: ["common"],
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  // <html lang> нужен скринридерам и браузеру: обновляем сразу и при смене языка
  document.documentElement.lang = locale;
  i18n.on("languageChanged", (next) => {
    document.documentElement.lang = next;
  });
  settings.on("locale", (next) => void i18n.changeLanguage(next));
}

export function useT() {
  return useTranslation("common").t;
}

export function useLocale(): SupportedLocale {
  return useSetting("locale");
}
