import { useState } from "react";

import {
  useErrorText,
  useRemoveLibraryMutation,
  useUpsertLibraryMutation,
  type LibraryState,
} from "@/api";
import { LIBRARY_STATUSES, type LibraryStatus } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import { Select } from "@/ui/Select";

type LibraryPanelProps = {
  titleId: number;
  entry: LibraryState | null;
};

function parseScore(value: string): number | null {
  return value === "" ? null : Number(value);
}

/** Статус и оценка применяются сразу: отдельной кнопки «сохранить» нет. */
export function LibraryPanel({ titleId, entry }: LibraryPanelProps) {
  const t = useT();
  const [status, setStatus] = useState<LibraryStatus>(entry?.status ?? "planned");
  const [score, setScore] = useState(entry?.score === null || entry?.score === undefined
    ? ""
    : String(entry.score));

  const upsert = useUpsertLibraryMutation(titleId);
  const remove = useRemoveLibraryMutation(titleId);
  const errorText = useErrorText(upsert.error ?? remove.error);
  const pending = upsert.isPending || remove.isPending;

  function revertToEntry(): void {
    setStatus(entry?.status ?? "planned");
    setScore(
      entry?.score === null || entry?.score === undefined ? "" : String(entry.score),
    );
  }

  function save(nextStatus: LibraryStatus, nextScore: string): void {
    // Ошибка прошлого удаления не относится к текущему действию
    remove.reset();
    upsert.mutate(
      { status: nextStatus, score: parseScore(nextScore) },
      { onError: revertToEntry },
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1.5">
          <span className="text-xs text-muted-foreground">{t("library.status")}</span>
          <Select
            value={status}
            disabled={pending}
            className="w-44"
            onChange={(event) => {
              const next = event.target.value as LibraryStatus;
              setStatus(next);
              if (entry) save(next, score);
            }}
          >
            {LIBRARY_STATUSES.map((value) => (
              <option key={value} value={value}>
                {t(`library.statuses.${value}`)}
              </option>
            ))}
          </Select>
        </label>

        <label className="flex flex-col gap-1.5">
          <span className="text-xs text-muted-foreground">{t("library.score")}</span>
          <Select
            value={score}
            disabled={pending}
            className="w-28"
            onChange={(event) => {
              const next = event.target.value;
              setScore(next);
              if (entry) save(status, next);
            }}
          >
            <option value="">{t("library.noScore")}</option>
            {Array.from({ length: 10 }, (_, index) => String(index + 1)).map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </Select>
        </label>

        {entry ? (
          <Button
            type="button"
            variant="destructive"
            disabled={pending}
            onClick={() => {
              upsert.reset();
              remove.mutate();
            }}
          >
            {t("library.remove")}
          </Button>
        ) : (
          <Button
            type="button"
            disabled={pending}
            onClick={() => save(status, score)}
          >
            {t("title.addToLibrary")}
          </Button>
        )}
      </div>

      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}
    </div>
  );
}
