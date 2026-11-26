import {
  BookmarkIcon,
  HeartIcon,
  PlusIcon,
} from "lucide-react";

import type { LibraryState } from "@/api";
import { LIBRARY_STATUSES, type LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui/shadcn/dropdown-menu";

type ReaderBookmarksProps = {
  library: LibraryState | null;
  pending: boolean;
  errorText: string | null;
  onAdd: () => void;
  onStatus: (status: LibraryStatus) => void;
  onToggleFavorite: () => void;
};

/** Быстрые действия библиотеки в читалке: избранное и статус. */
export function ReaderBookmarks({
  library,
  pending,
  errorText,
  onAdd,
  onStatus,
  onToggleFavorite,
}: ReaderBookmarksProps) {
  const t = useT();
  const favorite = library?.favorite ?? false;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="secondary"
            size="icon-lg"
            className="relative rounded-full shadow-sm"
            aria-label={t("reader.bookmarks")}
          />
        }
      >
        <BookmarkIcon className={favorite ? "fill-current" : undefined} />
        {library !== null && (
          <span
            aria-hidden
            className="absolute top-1 right-1 size-1.5 rounded-full bg-primary"
          />
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="end" className="w-60 p-1.5">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("reader.librarySection")}</DropdownMenuLabel>
          {library === null ? (
            <DropdownMenuItem disabled={pending} onClick={onAdd}>
              <PlusIcon />
              <span>{t("title.addToLibrary")}</span>
            </DropdownMenuItem>
          ) : (
            <DropdownMenuCheckboxItem
              checked={favorite}
              disabled={pending}
              onCheckedChange={onToggleFavorite}
            >
              <HeartIcon className={favorite ? "fill-current" : undefined} />
              <span>{t("library.favorite")}</span>
            </DropdownMenuCheckboxItem>
          )}
        </DropdownMenuGroup>

        {library !== null && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuRadioGroup
              value={library.status}
              onValueChange={(value) => onStatus(value as LibraryStatus)}
            >
              <DropdownMenuLabel>{t("library.status")}</DropdownMenuLabel>
              {LIBRARY_STATUSES.map((status) => (
                <DropdownMenuRadioItem
                  key={status}
                  value={status}
                  disabled={pending}
                >
                  {t(`library.statuses.${status}`)}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </>
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
