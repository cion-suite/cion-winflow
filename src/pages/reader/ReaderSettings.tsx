import { useState } from "react";
import { SettingsIcon, XIcon } from "lucide-react";

import { useT } from "@/lib/i18n";
import {
  isReaderWidth,
  READER_WIDTHS,
} from "@/lib/reader";
import { settings, useSetting } from "@/lib/settings";
import { Button } from "@/ui/shadcn/button";
import { Drawer, DrawerContent, DrawerHeader, DrawerTitle, DrawerTrigger } from "@/ui/shadcn/drawer";
import { Field, FieldLabel } from "@/ui/shadcn/field";
import { useIsMobile } from "@/ui/shadcn/hooks/use-mobile";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/ui/shadcn/sheet";

type ReaderSettingsProps = {
  /** Текущая страница, 1-based. */
  page: number;
  total: number;
};

function SettingsBody({ page, total }: ReaderSettingsProps) {
  const t = useT();
  const width = useSetting("readerWidth");
  const options = READER_WIDTHS.map((value) => ({
    value,
    label: t(`reader.widths.${value}`),
  }));

  function handleWidth(next: string | null): void {
    if (isReaderWidth(next)) void settings.set("readerWidth", next);
  }

  return (
    <div className="flex flex-col gap-4">
      {total > 0 && (
        <p className="text-sm text-muted-foreground">
          {t("reader.pageCounter", { page, total })}
        </p>
      )}
      <Field>
        <FieldLabel>{t("reader.width")}</FieldLabel>
        <Select
          items={options}
          value={width}
          onValueChange={handleWidth}
        >
          <SelectTrigger className="w-full" aria-label={t("reader.width")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
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
