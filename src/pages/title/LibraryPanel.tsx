import { useState } from "react";
import { Trash2Icon } from "lucide-react";

import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";

type LibraryPanelProps = {
  pending: boolean;
  errorText: string | null;
  onRemove: () => void;
};

/**
 * Удаление из библиотеки. Статус (включая свои закладки) и оценка живут
 * выше: статус — под обложкой, оценка — рядом с рейтингом источника.
 */
export function LibraryPanel({
  pending,
  errorText,
  onRemove,
}: LibraryPanelProps) {
  const t = useT();
  const [confirmingRemove, setConfirmingRemove] = useState(false);

  return (
    <div className="flex flex-col gap-2">
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
