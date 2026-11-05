import type { ReactElement } from "react";
import { Navigate, Route, Routes } from "react-router";

import { AppsPage } from "@/pages/apps";
import { ChangelogPage } from "@/pages/changelog";
import { HomePage } from "@/pages/home";
import { NetworkPage } from "@/pages/network";
import { SettingsPage } from "@/pages/settings";
import { SystemPage } from "@/pages/system";
import { UtilsPage } from "@/pages/utils";
import { Layout } from "@/shell/layout/Layout";
import type { SectionPath } from "@/shell/sidebar";

// Ключ - раздел из SECTIONS: рассинхрон не скомпилируется
const PAGES: Record<Exclude<SectionPath, "/">, ReactElement> = {
  "/network": <NetworkPage />,
  "/apps": <AppsPage />,
  "/utils": <UtilsPage />,
  "/system": <SystemPage />,
  "/changelog": <ChangelogPage />,
  "/settings": <SettingsPage />,
};

export function AppRouter() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<HomePage />} />
        {Object.entries(PAGES).map(([path, element]) => (
          <Route key={path} path={path} element={element} />
        ))}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
