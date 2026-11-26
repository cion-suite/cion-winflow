import { useState } from "react";
import { SettingsIcon, XIcon } from "lucide-react";

import { useT } from "@/lib/i18n";
import { READER_WIDTHS } from "@/lib/reader";
import { settings, useSetting } from "@/lib/settings";
import { Button } from "@/ui/shadcn/button";
import { Drawer, DrawerContent, DrawerHeader, DrawerTitle, DrawerTrigger } from "@/ui/shadcn/drawer";
import { Field, FieldLabel } from "@/ui/shadcn/field";
import { useIsMobile } from "@/ui/shadcn/hooks/use-mobile";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/ui/shadcn/sheet";
import { Slider } from "@/ui/shadcn/slider";

type ReaderSettingsProps = {
  /** Текущая страница, 1-based. */
  page: number;
  total: number;
};

function SettingsBody({ page, total }: ReaderSettingsProps) {
  const t = useT();
  const width = useSetting("readerWidth");
  const widthIndex = Math.max(0, READER_WIDTHS.indexOf(width));

  function handleWidth(value: number | readonly number[]): void {
    const index = Array.isArray(value) ? value[0] : value;
    if (typeof index !== "number") return;
    const next = READER_WIDTHS[Math.round(index)];
    if (next !== undefined) void settings.set("readerWidth", next);
  }

  return (
    <div className="flex flex-col gap-4">
      {total > 0 && (
        <p className="text-sm text-muted-foreground">
          {t("reader.pageCounter", { page, total })}
        </p>
      )}
      <Field>
        <div className="flex items-center justify-between gap-3">
          <FieldLabel>{t("reader.width")}</FieldLabel>
          <span className="text-xs text-muted-foreground tabular-nums">
            {t(`reader.widths.${width}`)}
          </span>
        </div>
        <Slider
          min={0}
          max={READER_WIDTHS.length - 1}
          step={1}
          value={[widthIndex]}
          onValueChange={handleWidth}
          aria-label={t("reader.width")}
        />
        <div
          className="flex justify-between px-0.5 text-xs text-muted-foreground"
          aria-hidden
        >
          <span>{t("reader.widths.sm")}</span>
          <span>{t("reader.widths.full")}</span>
        </div>
      </Field>
    </div>
  );
}

/** Настройки читалки: снизу на телефоне, боковой панелью на широком экране. */
export function ReaderSettings({ page, total }: ReaderSettingsProps) {
  const t = useT();
  const isMobile = useIsMobile();
  const [open, setOpen] = useState(false);

  if (isMobile) {
    return (
      <Drawer
        open={open}
        onOpenChange={setOpen}
        swipeDirection="down"
        showSwipeHandle
      >
        <DrawerTrigger
          render={
            <Button
              variant="secondary"
              size="icon-lg"
              className="rounded-full shadow-sm"
              aria-label={t("reader.settings")}
            />
          }
        >
          <SettingsIcon />
        </DrawerTrigger>
        <DrawerContent>
          <DrawerHeader>
            <DrawerTitle>{t("reader.settings")}</DrawerTitle>
          </DrawerHeader>
          <div className="overflow-y-auto p-4 pt-2">
            <SettingsBody page={page} total={total} />
          </div>
        </DrawerContent>
      </Drawer>
    );
  }

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger
        render={
          <Button
            variant="secondary"
            size="icon-lg"
            className="rounded-full shadow-sm"
            aria-label={t("reader.settings")}
          />
        }
      >
        <SettingsIcon />
      </SheetTrigger>
      <SheetContent side="right" showCloseButton={false}>
        <SheetHeader className="flex-row items-center justify-between">
          <SheetTitle>{t("reader.settings")}</SheetTitle>
          <SheetClose
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t("common.close")}
              />
            }
          >
            <XIcon />
          </SheetClose>
        </SheetHeader>
        <div className="px-4 pb-4">
          <SettingsBody page={page} total={total} />
        </div>
      </SheetContent>
    </Sheet>
  );
}
