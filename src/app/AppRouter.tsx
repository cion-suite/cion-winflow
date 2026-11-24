import type { ReactElement } from "react";
import { Navigate, Route, Routes } from "react-router";

import { ChangelogPage } from "@/pages/changelog";
import { HomePage } from "@/pages/home";
import { ReaderPage } from "@/pages/reader";
import { SettingsPage } from "@/pages/settings";
import { TitlePage } from "@/pages/title";
import { WatchPage } from "@/pages/watch";
import { Layout } from "@/shell/layout/Layout";
import type { SectionPath } from "@/shell/sidebar";

// Ключ - раздел из SECTIONS: рассинхрон не скомпилируется
const PAGES: Record<Exclude<SectionPath, "/">, ReactElement> = {
  "/changelog": <ChangelogPage />,
  "/settings": <SettingsPage />,
};

/**
 * Параметрические маршруты живут вне SectionPath: меню остаётся статическим,
 * а активный пункт и заголовок определяются по префиксу пути.
 */
const PARAM_ROUTES = [
  { path: "/title/:id", element: <TitlePage /> },
  { path: "/reader/:titleId/:chapterKey", element: <ReaderPage /> },
  { path: "/watch/:titleId/:chapterKey", element: <WatchPage /> },
] as const satisfies readonly { path: string; element: ReactElement }[];

export function AppRouter() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<HomePage />} />
        {Object.entries(PAGES).map(([path, element]) => (
          <Route key={path} path={path} element={element} />
        ))}
        {PARAM_ROUTES.map(({ path, element }) => (
          <Route key={path} path={path} element={element} />
        ))}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
