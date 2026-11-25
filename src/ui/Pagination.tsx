import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";

type PaginationProps = {
  page: number;
  pages: number;
  onPageChange: (page: number) => void;
};

/** Кнопки «назад/вперёд» для постраничных списков. */
export function Pagination({ page, pages, onPageChange }: PaginationProps) {
  const t = useT();
  if (pages <= 1) return null;

  return (
    <div className="flex items-center justify-between gap-2 pt-2">
      <Button
        type="button"
        variant="outline"
        disabled={page <= 1}
        onClick={() => onPageChange(Math.max(1, page - 1))}
      >
        {t("library.previous")}
      </Button>
      <span className="text-sm text-muted-foreground">
        {t("library.page", { page, pages })}
      </span>
      <Button
        type="button"
        variant="outline"
        disabled={page >= pages}
        onClick={() => onPageChange(page + 1)}
      >
        {t("library.next")}
      </Button>
    </div>
  );
}
