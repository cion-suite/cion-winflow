import { Navigate, Route, Routes } from "react-router";
import { GreetPage } from "@/pages/greet";
import { Layout } from "@/shell/layout/Layout";

export function AppRouter() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<GreetPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
