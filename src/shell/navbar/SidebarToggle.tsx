import { useEffect } from "react";
import { PanelLeftCloseIcon, PanelLeftOpenIcon } from "lucide-react";

import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import { useSidebar } from "@/ui/shadcn/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/shadcn/tooltip";

export function SidebarToggle() {
  const t = useT();
  const { toggleSidebar, state } = useSidebar();
  const Icon = state === "expanded" ? PanelLeftCloseIcon : PanelLeftOpenIcon;

  // SidebarProvider ловит клавишу "b": на кириллице Ctrl+B молчит
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code !== "KeyB" || !(event.ctrlKey || event.metaKey)) return;
      // Провайдер сам ловит только строчную "b"; остальные раскладки и CapsLock - здесь
      if (event.key === "b") return;
      event.preventDefault();
      toggleSidebar();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [toggleSidebar]);

  return (
    <Tooltip>
      <TooltipTrigger render={<Button variant="ghost" size="icon-sm" onClick={toggleSidebar} />}>
        <Icon />
        <span className="sr-only">{t("nav.toggleSidebar")}</span>
      </TooltipTrigger>
      <TooltipContent side="bottom">{t("nav.toggleSidebar")}</TooltipContent>
    </Tooltip>
  );
}
