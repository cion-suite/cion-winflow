import type { ReactElement } from "react";
import { Navigate, Route, Routes } from "react-router";

import { AdminPage } from "@/pages/admin";
import { ChangelogPage } from "@/pages/changelog";
import { LibraryPage } from "@/pages/library";
import { LoginPage } from "@/pages/login";
import { ReaderPage } from "@/pages/reader";
import { RegisterPage } from "@/pages/register";
import { SettingsPage } from "@/pages/settings";
import { TitlePage } from "@/pages/title";
import { WatchPage } from "@/pages/watch";
import { Layout } from "@/shell/layout/Layout";
import { RequireAuth } from "./RequireAuth";

/** Статические экраны: меню шапки берёт подписи из lib/nav.ts. */
const PAGES = [
  { path: "/admin", element: <AdminPage /> },
  { path: "/changelog", element: <ChangelogPage /> },
  { path: "/settings", element: <SettingsPage /> },
] as const satisfies readonly { path: string; element: ReactElement }[];

/**
 * Параметрические маршруты живут отдельно: меню остаётся статическим,
 * а заголовок экрана задаёт сам экран.
 */
const PARAM_ROUTES = [
  { path: "/title/:id", element: <TitlePage /> },
  { path: "/watch/:titleId/:chapterKey", element: <WatchPage /> },
] as const satisfies readonly { path: string; element: ReactElement }[];

export function AppRouter() {
  return (
    <Routes>
      {/* Экраны входа живут вне Layout: шапка гостю не нужна */}
      <Route path="/login" element={<LoginPage />} />
      <Route path="/register" element={<RegisterPage />} />

      <Route element={<RequireAuth />}>
        <Route element={<Layout />}>
          <Route index element={<LibraryPage />} />
          {/* Старая ссылка на отдельный экран библиотеки */}
          <Route path="/library" element={<Navigate to="/" replace />} />
          {PAGES.map(({ path, element }) => (
            <Route key={path} path={path} element={element} />
          ))}
          {PARAM_ROUTES.map(({ path, element }) => (
            <Route key={path} path={path} element={element} />
          ))}
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>

        {/* Читалка — во весь экран, без шапки */}
        <Route path="/reader/:titleId/:chapterKey" element={<ReaderPage />} />
      </Route>
    </Routes>
  );
}
