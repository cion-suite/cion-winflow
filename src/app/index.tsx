import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { initI18n } from "@/lib/i18n";
import { initSettings } from "@/lib/settings";
import { AppProvider } from "./AppProvider";
import { AppRouter } from "./AppRouter";
import "./styles/index.css";

// Настройки и язык готовы до первого рендера: без этого язык интерфейса мигал бы
async function bootstrap(): Promise<void> {
  await initSettings();
  initI18n();

  const rootElement = document.getElementById("root");
  if (!rootElement) {
    throw new Error("Element #root not found in index.html");
  }

  createRoot(rootElement).render(
    <StrictMode>
      <AppProvider>
        <AppRouter />
      </AppProvider>
    </StrictMode>,
  );
}

void bootstrap();
