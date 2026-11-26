import { useState } from "react";
import { BookmarkIcon, CheckIcon, PlusIcon, Trash2Icon } from "lucide-react";

import type { LibraryState, LibraryTab } from "@/api";
import { LIBRARY_STATUSES, type LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/ui/shadcn/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";

type LibraryPanelProps = {
  entry: LibraryState | null;
  tabs: LibraryTab[];
  pending: boolean;
  errorText: string | null;
  onAdd: () => void;
  onStatus: (status: LibraryStatus) => void;
  onToggleTab: (tabId: number) => void;
  onRemove: () => void;
};

/**
 * Статус, закладки и удаление живут под обложкой. Оценка — рядом с рейтингом
 * источника: так две шкалы не путаются.
 */
export function LibraryPanel({
  entry,
  tabs,
  pending,
  errorText,
  onAdd,
  onStatus,
  onToggleTab,
  onRemove,
}: LibraryPanelProps) {
  const t = useT();
  const [confirmingRemove, setConfirmingRemove] = useState(false);
  const statusOptions = LIBRARY_STATUSES.map((status) => ({
    value: status,
    label: t(`library.statuses.${status}`),
  }));

  if (!entry) {
    return (
      <div className="flex flex-col gap-2">
        <Button
          type="button"
          variant="outline"
          className="w-full"
          disabled={pending}
          onClick={onAdd}
        >
          <PlusIcon data-icon="inline-start" />
          {t("title.addToLibrary")}
        </Button>
        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <label className="flex flex-col gap-1.5">
        <span className="text-xs text-muted-foreground">{t("library.status")}</span>
        <Select
          items={statusOptions}
          value={entry.status}
          disabled={pending}
          onValueChange={(value) => onStatus(value as LibraryStatus)}
        >
          <SelectTrigger className="w-full" aria-label={t("library.status")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {statusOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>

      {tabs.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={<Button type="button" variant="outline" className="w-full" />}
          >
            <BookmarkIcon data-icon="inline-start" />
            {t("library.customTabs")}
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-(--anchor-width)">
            <DropdownMenuGroup>
              <DropdownMenuLabel>{t("library.customTabs")}</DropdownMenuLabel>
              {tabs.map((tab) => {
                const selected = entry.tab_ids.includes(tab.id);
                return (
                  <DropdownMenuItem
                    key={tab.id}
                    disabled={pending}
                    onClick={() => onToggleTab(tab.id)}
                  >
                    {selected ? (
                      <CheckIcon aria-hidden />
                    ) : (
                      <span className="size-4" aria-hidden />
                    )}
                    <span className="truncate">{tab.name}</span>
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      {confirmingRemove ? (
        <Button
          type="button"
          variant="destructive"
          disabled={pending}
          onClick={onRemove}
        >
          <Trash2Icon data-icon="inline-start" />
          {t("library.confirmRemove")}
        </Button>
      ) : (
        <Button
          type="button"
          variant="ghost"
          disabled={pending}
          onClick={() => setConfirmingRemove(true)}
        >
          {t("library.remove")}
        </Button>
      )}

      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}
    </div>
  );
}
