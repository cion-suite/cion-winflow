import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { AppProvider } from "./AppProvider";
import { AppRouter } from "./AppRouter";
import "./styles/index.css";

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
