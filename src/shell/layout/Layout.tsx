import { Outlet } from "react-router";

import { settings, useSetting } from "@/lib/settings";
import { AppSidebar } from "@/shell/sidebar";
import { Navbar } from "@/shell/navbar/Navbar";
import { SidebarInset, SidebarProvider } from "@/ui/shadcn/sidebar";

export function Layout() {
  const sidebarOpen = useSetting("sidebarOpen");

  return (
    <SidebarProvider
      open={sidebarOpen}
      onOpenChange={(open) => void settings.set("sidebarOpen", open)}
    >
      <AppSidebar />
      <SidebarInset className="h-svh overflow-hidden">
        <Navbar />
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-6">
          <Outlet />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
