import { Outlet } from "react-router";
import { Navbar } from "@/shell/navbar/Navbar";

export function Layout() {
  return (
    <div className="flex h-svh flex-col overflow-hidden">
      <Navbar />
      <main className="flex min-h-0 flex-1 flex-col overflow-y-auto p-6">
        <Outlet />
      </main>
    </div>
  );
}
