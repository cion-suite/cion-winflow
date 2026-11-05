import { Link, useLocation } from "react-router";

import { useT } from "@/lib/i18n";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from "@/ui/shadcn/sidebar";
import { SECTIONS, SIDEBAR_GROUPS, sectionForPath, type SectionGroup } from "./sections";

function SectionMenu({ group, labelKey }: { group: SectionGroup; labelKey?: string }) {
  const t = useT();
  const { pathname } = useLocation();
  const { setOpenMobile } = useSidebar();
  const current = sectionForPath(pathname);

  return (
    <>
      {/* В свёрнутом до иконок сайдбаре заголовок скрывает сам SidebarGroupLabel */}
      {labelKey && <SidebarGroupLabel>{t(labelKey)}</SidebarGroupLabel>}
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
    </>
  );
}

export function AppSidebar() {
  return (
    <Sidebar collapsible="icon">
      <SidebarContent>
        {/* SidebarGroup даёт горизонтальный отступ, как у SidebarFooter */}
        {SIDEBAR_GROUPS.map((group) => (
          <SidebarGroup key={group.id}>
            <SectionMenu group={group.id} labelKey={group.i18nKey} />
          </SidebarGroup>
        ))}
      </SidebarContent>
      <SidebarFooter>
        <SectionMenu group="footer" />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
