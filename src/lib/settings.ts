import { useSyncExternalStore } from "react";

import { DEFAULT_LOCALE, SUPPORTED_LOCALES, type SupportedLocale } from "./i18n/resources";
import {
  DEFAULT_READER_WIDTH,
  isReaderWidth,
  type ReaderWidth,
} from "./reader";

/**
 * Настройки приложения. Хранятся в localStorage; серверные настройки появятся
 * вместе с профилем пользователя. Читаются до первого рендера, поэтому язык
 * известен i18n сразу.
 */

export type SettingKey = "locale" | "readerWidth" | "titleLangs";

type SettingsMap = {
  locale: SupportedLocale;
  /** Ширина колонки изображений в читалке. */
  readerWidth: ReaderWidth;
  /** Язык просмотра по id тайтла: выбор переживает переходы и перезагрузку. */
  titleLangs: Record<string, string>;
};

const BROWSER_KEY = "cion-reader:settings";

// Язык системы в первый запуск; сохранённый выбор важнее
function initialLocale(): SupportedLocale {
  const preferred = navigator.language.slice(0, 2).toLowerCase();
  return SUPPORTED_LOCALES.find((code) => code === preferred) ?? DEFAULT_LOCALE;
}

const DEFAULTS: SettingsMap = {
  locale: initialLocale(),
  readerWidth: DEFAULT_READER_WIDTH,
  titleLangs: {},
};

const values: SettingsMap = { ...DEFAULTS };
const listeners = new Map<SettingKey, Set<() => void>>();

const isValid = {
  locale: (value: unknown): value is SupportedLocale =>
    typeof value === "string" && SUPPORTED_LOCALES.some((code) => code === value),
  readerWidth: isReaderWidth,
  titleLangs: (value: unknown): value is Record<string, string> =>
    typeof value === "object" &&
    value !== null &&
    Object.values(value).every((lang) => typeof lang === "string"),
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
  if (isValid.readerWidth(record.readerWidth)) values.readerWidth = record.readerWidth;
  if (isValid.titleLangs(record.titleLangs)) {
    values.titleLangs = { ...record.titleLangs };
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
