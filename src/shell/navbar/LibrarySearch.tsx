import { type FormEvent, useEffect, useRef, useState } from "react";
import { SearchIcon } from "lucide-react";
import { Link, useNavigate } from "react-router";

import { titleCoverUrl, useLibraryQuery } from "@/api";
import { useT } from "@/lib/i18n";
import { titlePath } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { CoverImage } from "@/ui/CoverImage";
import { Input } from "@/ui/shadcn/input";
import { Spinner } from "@/ui/shadcn/spinner";

const DEBOUNCE_MS = 250;
const RESULT_LIMIT = 6;

/** Быстрый переход по уже добавленным тайтлам: поиск только по своей библиотеке. */
export function LibrarySearch() {
  const t = useT();
  const navigate = useNavigate();
  const [value, setValue] = useState("");
  const [debounced, setDebounced] = useState("");
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value.trim()), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [value]);

  const query = useLibraryQuery(
    { q: debounced, pageSize: RESULT_LIMIT },
    { enabled: debounced.length > 0 },
  );

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, []);

  const items = debounced.length > 0 ? (query.data?.items ?? []) : [];
  const showPanel = open && debounced.length > 0;

  function openTitle(titleId: number): void {
    setOpen(false);
    setValue("");
    setDebounced("");
    void navigate(titlePath(titleId));
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const first = items[0];
    if (first) openTitle(first.title_id);
  }

  return (
    <div ref={containerRef} className="relative w-full">
      <form role="search" onSubmit={handleSubmit}>
        <div className="relative">
          <SearchIcon
            aria-hidden
            className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            type="search"
            value={value}
            placeholder={t("library.searchPlaceholder")}
            aria-label={t("library.searchPlaceholder")}
            className="bg-muted/50 pl-8"
            onChange={(event) => {
              setValue(event.target.value);
              setOpen(true);
            }}
            onFocus={() => setOpen(true)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setOpen(false);
            }}
          />
          {query.isFetching && (
            <Spinner
              className="absolute top-1/2 right-2.5 -translate-y-1/2 text-muted-foreground"
              aria-label={t("library.searching")}
            />
          )}
        </div>
      </form>

      {showPanel && (
        <div className="absolute inset-x-0 top-full z-40 mt-2 overflow-hidden rounded-xl border bg-popover text-popover-foreground shadow-md">
          {query.isPending ? (
            <p className="p-3 text-sm text-muted-foreground">{t("library.searching")}</p>
          ) : items.length === 0 ? (
            <p className="p-3 text-sm text-muted-foreground">{t("library.searchEmpty")}</p>
          ) : (
            <ul className="flex flex-col p-1">
              {items.map((item) => (
                <li key={item.title_id}>
                  <Link
                    to={titlePath(item.title_id)}
                    className={cn(
                      "flex items-center gap-3 rounded-lg px-2 py-1.5 outline-none",
                      "hover:bg-muted focus-visible:bg-muted",
                    )}
                    onClick={(event) => {
                      event.preventDefault();
                      openTitle(item.title_id);
                    }}
                  >
                    <CoverImage
                      src={
                        item.title.has_cover ? titleCoverUrl(item.title, "low") : null
                      }
                      alt={item.title.name}
                      className="h-12 w-8 shrink-0 rounded-md"
                    />
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate text-sm font-medium">
                        {item.title.name}
                      </span>
                      <span className="truncate text-xs text-muted-foreground">
                        {t(`catalog.kinds.${item.title.kind}`)}
                        {item.title.year !== null ? ` · ${item.title.year}` : ""}
                      </span>
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
