import { useSyncExternalStore } from "react";

import { DEFAULT_LOCALE, SUPPORTED_LOCALES, type SupportedLocale } from "./i18n/resources";

/**
 * Настройки приложения. Хранятся в localStorage; серверные настройки появятся
 * вместе с профилем пользователя. Читаются до первого рендера, поэтому язык
 * известен i18n сразу.
 */

export type SettingKey = "locale" | "sidebarOpen";

type SettingsMap = {
  locale: SupportedLocale;
  sidebarOpen: boolean;
};

const BROWSER_KEY = "cion-reader:settings";

// Язык системы в первый запуск; сохранённый выбор важнее
function initialLocale(): SupportedLocale {
  const preferred = navigator.language.slice(0, 2).toLowerCase();
  return SUPPORTED_LOCALES.find((code) => code === preferred) ?? DEFAULT_LOCALE;
}

const DEFAULTS: SettingsMap = {
  locale: initialLocale(),
  sidebarOpen: true,
};

const values: SettingsMap = { ...DEFAULTS };
const listeners = new Map<SettingKey, Set<() => void>>();

const isValid = {
  locale: (value: unknown): value is SupportedLocale =>
    typeof value === "string" && SUPPORTED_LOCALES.some((code) => code === value),
  sidebarOpen: (value: unknown): value is boolean => typeof value === "boolean",
};

function readBrowserSettings(): unknown {
  try {
    const raw = localStorage.getItem(BROWSER_KEY);
    return raw === null ? null : JSON.parse(raw);
  } catch (error) {
    // Битый JSON не должен откатывать настройки молча: причина видна в консоли
    console.error("Failed to read settings from localStorage", error);
    return null;
  }
}

function notify(key: SettingKey): void {
  listeners.get(key)?.forEach((listener) => listener());
}

function subscribe(key: SettingKey, listener: () => void): () => void {
  const group = listeners.get(key) ?? new Set<() => void>();
  listeners.set(key, group);
  group.add(listener);
  return () => {
    group.delete(listener);
  };
}

function persist(key: SettingKey): void {
  try {
    localStorage.setItem(BROWSER_KEY, JSON.stringify(values));
  } catch (error) {
    console.error(`Failed to save setting "${key}"`, error);
  }
}

/** Загружает сохранённые настройки. Вызывать до initI18n и рендера. */
export function initSettings(): void {
  const saved = readBrowserSettings();
  if (typeof saved !== "object" || saved === null) return;

  const record = saved as Record<string, unknown>;
  if (isValid.locale(record.locale)) values.locale = record.locale;
  if (isValid.sidebarOpen(record.sidebarOpen)) values.sidebarOpen = record.sidebarOpen;
}

export const settings = {
  get<K extends SettingKey>(key: K): SettingsMap[K] {
    return values[key];
  },

  async set<K extends SettingKey>(key: K, value: SettingsMap[K]): Promise<void> {
    if (values[key] === value) return;
    values[key] = value;
    notify(key);
    persist(key);
  },

  /** Подписка на изменение одного ключа; возвращает отписку. */
  on<K extends SettingKey>(key: K, listener: (value: SettingsMap[K]) => void): () => void {
    return subscribe(key, () => listener(values[key]));
  },
};

export function useSetting<K extends SettingKey>(key: K): SettingsMap[K] {
  return useSyncExternalStore(
    (listener) => subscribe(key, listener),
    () => values[key],
    () => DEFAULTS[key],
  );
}
