import { LanguagesIcon } from "lucide-react";

import { SUPPORTED_LOCALES, useLocale, useT } from "@/lib/i18n";
import { settings } from "@/lib/settings";
import { Button } from "@/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/ui/shadcn/dropdown-menu";

export function LocaleMenu() {
  const t = useT();
  const locale = useLocale();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" />}>
        <LanguagesIcon />
        <span className="sr-only">{t("locale.toggle")}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("shell.navbar.language")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={locale}
            onValueChange={(value) => {
              const next = SUPPORTED_LOCALES.find((code) => code === value);
              if (next) void settings.set("locale", next);
            }}
          >
            {SUPPORTED_LOCALES.map((value) => (
              <DropdownMenuRadioItem key={value} value={value} closeOnClick>
                {t(`locale.name.${value}`)}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
