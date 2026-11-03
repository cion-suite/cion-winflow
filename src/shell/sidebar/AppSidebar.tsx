import { Link, useLocation } from "react-router";

import { useT } from "@/lib/i18n";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from "@/ui/shadcn/sidebar";
import { SECTIONS, sectionForPath, type SectionGroup } from "./sections";

function SectionMenu({ group }: { group: SectionGroup }) {
  const t = useT();
  const { pathname } = useLocation();
  const { setOpenMobile } = useSidebar();
  const current = sectionForPath(pathname);

  return (
    <SidebarMenu>
      {SECTIONS.filter((section) => section.group === group).map((section) => (
        <SidebarMenuItem key={section.path}>
          <SidebarMenuButton
            render={<Link to={section.path} onClick={() => setOpenMobile(false)} />}
            isActive={current === section}
            tooltip={t(section.i18nKey)}
          >
            <section.icon />
            <span>{t(section.i18nKey)}</span>
          </SidebarMenuButton>
        </SidebarMenuItem>
      ))}
    </SidebarMenu>
  );
}

export function AppSidebar() {
  return (
    <Sidebar collapsible="icon">
      <SidebarContent>
        {/* SidebarGroup даёт горизонтальный отступ, как у SidebarFooter */}
        <SidebarGroup>
          <SectionMenu group="main" />
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <SectionMenu group="footer" />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
