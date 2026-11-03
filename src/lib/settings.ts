import { useSyncExternalStore } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { load, type Store } from "@tauri-apps/plugin-store";

import { DEFAULT_LOCALE, SUPPORTED_LOCALES, type SupportedLocale } from "./i18n/resources";

/**
 * Настройки приложения: хранятся в Tauri store (файл settings.json в данных
 * приложения), а в обычном браузере - в localStorage. Читаются до первого
 * рендера, поэтому язык известен i18n сразу.
 */

export type SettingKey = "locale" | "sidebarOpen";

type SettingsMap = {
  locale: SupportedLocale;
  sidebarOpen: boolean;
};

const STORE_FILE = "settings.json";
const BROWSER_KEY = "cc-tweaks:settings";

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
let store: Store | null = null;

const isValid = {
  locale: (value: unknown): value is SupportedLocale =>
    typeof value === "string" && SUPPORTED_LOCALES.some((code) => code === value),
  sidebarOpen: (value: unknown): value is boolean => typeof value === "boolean",
};

function readBrowserSettings(): unknown {
  try {
    const raw = localStorage.getItem(BROWSER_KEY);
    return raw === null ? null : JSON.parse(raw);
  } catch {
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

async function persist<K extends SettingKey>(key: K, value: SettingsMap[K]): Promise<void> {
  try {
    if (store) {
      await store.set(key, value);
      return;
    }
    localStorage.setItem(BROWSER_KEY, JSON.stringify(values));
  } catch (error) {
    console.error(`Failed to save setting "${key}"`, error);
  }
}

/** Загружает сохранённые настройки. Вызывать до initI18n и рендера. */
export async function initSettings(): Promise<void> {
  try {
    if (!isTauri()) {
      const saved = readBrowserSettings();
      if (typeof saved === "object" && saved !== null) {
        const record = saved as Record<string, unknown>;
        if (isValid.locale(record.locale)) values.locale = record.locale;
        if (isValid.sidebarOpen(record.sidebarOpen)) values.sidebarOpen = record.sidebarOpen;
      }
      return;
    }

    store = await load(STORE_FILE, { autoSave: 100 });
    const locale: unknown = await store.get("locale");
    if (isValid.locale(locale)) values.locale = locale;
    const sidebarOpen: unknown = await store.get("sidebarOpen");
    if (isValid.sidebarOpen(sidebarOpen)) values.sidebarOpen = sidebarOpen;
  } catch (error) {
    // Приложение продолжает работать на значениях по умолчанию
    console.error("Failed to load settings", error);
  }
}

export const settings = {
  get<K extends SettingKey>(key: K): SettingsMap[K] {
    return values[key];
  },

  async set<K extends SettingKey>(key: K, value: SettingsMap[K]): Promise<void> {
    if (values[key] === value) return;
    values[key] = value;
    notify(key);
    await persist(key, value);
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
