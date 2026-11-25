import { Outlet } from "react-router";

import { Navbar } from "@/shell/navbar/Navbar";

export function Layout() {
  return (
    <div className="flex h-svh flex-col overflow-hidden bg-background">
      <Navbar />
      <main className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-7xl p-4 sm:p-6">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
