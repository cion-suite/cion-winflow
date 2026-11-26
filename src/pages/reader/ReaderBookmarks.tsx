import { BookmarkIcon, PlusIcon } from "lucide-react";

import type { LibraryState, LibraryTab } from "@/api";
import {
  customStatusValue,
  customTabIdFromValue,
  LIBRARY_STATUSES,
  type LibraryStatus,
} from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui/shadcn/dropdown-menu";

type ReaderBookmarksProps = {
  library: LibraryState | null;
  tabs: LibraryTab[];
  pending: boolean;
  errorText: string | null;
  onAdd: () => void;
  onStatus: (status: LibraryStatus) => void;
  onCustomStatus: (tabId: number) => void;
  /** icon — нижняя панель телефона, wide — кнопка с подписью в шапке. */
  variant?: "icon" | "wide";
};

/** Статус библиотеки в читалке: встроенные и свои закладки одним списком. */
export function ReaderBookmarks({
  library,
  tabs,
  pending,
  errorText,
  onAdd,
  onStatus,
  onCustomStatus,
  variant = "icon",
}: ReaderBookmarksProps) {
  const t = useT();
  const selectedCustomTabId = library?.custom_tab_id ?? null;
  const selectedCustomTab =
    selectedCustomTabId === null
      ? undefined
      : tabs.find((tab) => tab.id === selectedCustomTabId);
  const customSelected = selectedCustomTab !== undefined;
  const selected = selectedCustomTab
    ? customStatusValue(selectedCustomTab.id)
    : (library?.status ?? "planned");

  function handleSelect(value: string): void {
    const tabId = customTabIdFromValue(value);
    if (tabId !== null) onCustomStatus(tabId);
    else onStatus(value as LibraryStatus);
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          variant === "wide" ? (
            <Button
              variant="secondary"
              size="lg"
              aria-label={t("reader.bookmarks")}
            />
          ) : (
            <Button
              variant="secondary"
              size="icon-lg"
              className="relative rounded-full shadow-sm"
              aria-label={t("reader.bookmarks")}
            />
          )
        }
      >
        <BookmarkIcon
          data-icon={variant === "wide" ? "inline-start" : undefined}
          className={cn(customSelected && "fill-current")}
        />
        {variant === "wide" && <span>{t("reader.bookmarks")}</span>}
        {variant === "icon" && library !== null && (
          <span
            aria-hidden
            className="absolute top-1 right-1 size-1.5 rounded-full bg-primary"
          />
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side={variant === "wide" ? "bottom" : "top"}
        align="end"
        className="w-60 p-1.5"
      >
        {library === null ? (
          <DropdownMenuGroup>
            <DropdownMenuLabel>{t("library.status")}</DropdownMenuLabel>
            {/* Меню не закрываем: иначе ошибка upsert придёт после unmount
                и её будет негде показать. */}
            <DropdownMenuItem
              closeOnClick={false}
              disabled={pending}
              onClick={onAdd}
            >
              <PlusIcon aria-hidden />
              <span>{t("title.addToLibrary")}</span>
            </DropdownMenuItem>
          </DropdownMenuGroup>
        ) : (
          <DropdownMenuRadioGroup value={selected} onValueChange={handleSelect}>
            <DropdownMenuLabel>{t("library.status")}</DropdownMenuLabel>
            {LIBRARY_STATUSES.map((status) => (
              <DropdownMenuRadioItem
                key={status}
                value={status}
                closeOnClick={false}
                disabled={pending}
              >
                {t(`library.statuses.${status}`)}
              </DropdownMenuRadioItem>
            ))}
            {tabs.length > 0 && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>{t("library.customTabs")}</DropdownMenuLabel>
                {tabs.map((tab) => (
                  <DropdownMenuRadioItem
                    key={tab.id}
                    value={customStatusValue(tab.id)}
                    closeOnClick={false}
                    disabled={pending}
                  >
                    {tab.name}
                  </DropdownMenuRadioItem>
                ))}
              </>
            )}
          </DropdownMenuRadioGroup>
        )}

        {errorText && (
          <>
            <DropdownMenuSeparator />
            <p role="alert" className="px-1.5 py-1 text-xs text-destructive">
              {errorText}
            </p>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
