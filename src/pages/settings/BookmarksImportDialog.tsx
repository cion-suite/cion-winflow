import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { FileJsonIcon, UploadIcon } from "lucide-react";

import {
  invalidateAfterBookmarkImport,
  useBookmarkImportMutation,
  useBookmarkImportQuery,
  useErrorText,
  type BookmarkImportJob,
} from "@/api";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui/shadcn/dialog";
import {
  Progress,
  ProgressLabel,
  ProgressValue,
} from "@/ui/shadcn/progress";
import { Spinner } from "@/ui/shadcn/spinner";

type BookmarksImportDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/** Результат старше часа уже не показываем: это не текущий импорт. */
const RESULT_TTL_SECONDS = 60 * 60;

function isRecent(job: BookmarkImportJob): boolean {
  return (
    job.finished_at === null ||
    Date.now() / 1000 - job.finished_at < RESULT_TTL_SECONDS
  );
}

/**
 * Импорт закладок из JSON-экспорта. Сервер разбирает файл и запускает
 * фоновое задание; диалог опрашивает его и обновляет библиотеку по
 * завершении, поэтому окно можно закрыть.
 */
export function BookmarksImportDialog({
  open,
  onOpenChange,
}: BookmarksImportDialogProps) {
  const t = useT();
  const queryClient = useQueryClient();
  const latest = useBookmarkImportQuery();
  const start = useBookmarkImportMutation();
  const [file, setFile] = useState<File | null>(null);
  const [choosing, setChoosing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const invalidatedJob = useRef<string | null>(null);

  const job = latest.data?.job ?? null;
  const errorText = useErrorText(start.error ?? latest.error);
  const running = job?.state === "running";
  const formVisible = choosing || job === null || !isRecent(job);

  // Завершение задания обновляет и профиль, и библиотеку. Инвалидируем
  // один раз на задание: опрос прекращается сразу после running.
  useEffect(() => {
    if (job && job.state !== "running" && isRecent(job)) {
      if (invalidatedJob.current !== job.id) {
        invalidatedJob.current = job.id;
        invalidateAfterBookmarkImport(queryClient);
      }
    }
  }, [job, queryClient]);

  function handleOpenChange(next: boolean): void {
    if (!next) {
      setFile(null);
      setChoosing(false);
      start.reset();
    }
    onOpenChange(next);
  }

  function handleFile(event: ChangeEvent<HTMLInputElement>): void {
    const selected = event.target.files?.[0] ?? null;
    // Иначе повторный выбор того же файла не вызовет onChange
    event.target.value = "";
    setFile(selected);
  }

  function handleStart(): void {
    if (!file || start.isPending) return;
    start.mutate(file, {
      onSuccess: () => {
        setChoosing(false);
        setFile(null);
      },
    });
  }

  function importAgain(): void {
    start.reset();
    setFile(null);
    setChoosing(true);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("profile.importBookmarks.title")}</DialogTitle>
          <DialogDescription>
            {t("profile.importBookmarks.description")}
          </DialogDescription>
        </DialogHeader>

        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}

        {latest.isPending ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {t("profile.importBookmarks.loading")}
          </div>
        ) : running && job ? (
          <RunningImport job={job} onClose={() => handleOpenChange(false)} />
        ) : formVisible || !job ? (
          <form
            className="flex flex-col gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              handleStart();
            }}
          >
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => inputRef.current?.click()}
              >
                <UploadIcon aria-hidden />
                {t("profile.importBookmarks.chooseFile")}
              </Button>
              <span className="min-w-0 truncate text-xs text-muted-foreground">
                {file?.name ?? t("profile.importBookmarks.noFile")}
              </span>
            </div>
            {/* Поле скрыто и не должно получать первый фокус диалога;
                файл выбирается видимой кнопкой выше. */}
            <input
              ref={inputRef}
              type="file"
              accept=".json,application/json"
              aria-label={t("profile.importBookmarks.chooseFile")}
              tabIndex={-1}
              className="sr-only"
              onChange={handleFile}
            />

            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenChange(false)}
              >
                {t("profile.importBookmarks.close")}
              </Button>
              <Button type="submit" disabled={!file || start.isPending}>
                {start.isPending && <Spinner />}
                {start.isPending
                  ? t("profile.importBookmarks.starting")
                  : t("profile.importBookmarks.start")}
              </Button>
            </div>
          </form>
        ) : (
          <FinishedImport
            job={job}
            onAgain={importAgain}
            onClose={() => handleOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function RunningImport({
  job,
  onClose,
}: {
  job: BookmarkImportJob;
  onClose: () => void;
}) {
  const t = useT();
  const percent =
    job.total > 0 ? Math.round((job.processed / job.total) * 100) : 0;

  return (
    <div className="flex flex-col gap-3">
      <Progress value={percent}>
        <ProgressLabel>{t("profile.importBookmarks.runningTitle")}</ProgressLabel>
        <ProgressValue>
          {() =>
            t("profile.importBookmarks.progress", {
              processed: job.processed,
              total: job.total,
            })
          }
        </ProgressValue>
      </Progress>

      <p className="text-xs text-muted-foreground">
        {t("profile.importBookmarks.background")}
      </p>

      <ImportCounters job={job} />
      <ImportErrors job={job} />

      <div className="flex justify-end">
        <Button type="button" variant="outline" onClick={onClose}>
          {t("profile.importBookmarks.close")}
        </Button>
      </div>
    </div>
  );
}

function FinishedImport({
  job,
  onAgain,
  onClose,
}: {
  job: BookmarkImportJob;
  onAgain: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const hasErrors = job.failed > 0;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <FileJsonIcon aria-hidden className="size-4 text-muted-foreground" />
        <p className="text-sm font-medium">
          {hasErrors
            ? t("profile.importBookmarks.failedTitle")
            : t("profile.importBookmarks.completedTitle")}
        </p>
      </div>

      <p className="text-sm text-muted-foreground">
        {t("profile.importBookmarks.summary", {
          created: job.library_created,
          updated: job.library_updated,
          imported: job.titles_imported,
        })}
      </p>

      <ImportCounters job={job} />
      <ImportErrors job={job} />

      <div className="flex justify-end gap-2">
        <Button type="button" variant="outline" onClick={onAgain}>
          {t("profile.importBookmarks.again")}
        </Button>
        <Button type="button" onClick={onClose}>
          {t("profile.importBookmarks.close")}
        </Button>
      </div>
    </div>
  );
}

function ImportCounters({ job }: { job: BookmarkImportJob }) {
  const t = useT();

  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
      <span>
        {t("profile.importBookmarks.readMarked", {
          count: job.read_marked,
        })}
      </span>
      {job.failed > 0 && (
        <span className="text-destructive">
          {t("profile.importBookmarks.failedCount", { count: job.failed })}
        </span>
      )}
    </div>
  );
}

function ImportErrors({ job }: { job: BookmarkImportJob }) {
  const t = useT();
  if (job.errors.length === 0) return null;

  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs font-medium text-muted-foreground">
        {t("profile.importBookmarks.errors")}
      </p>
      <ul className="max-h-40 overflow-y-auto rounded-md bg-muted/50 p-2 text-xs">
        {job.errors.map((error, index) => (
          <li key={`${error.name}-${index}`} className="break-words">
            {error.name !== "" && (
              <span className="font-medium">{error.name}: </span>
            )}
            {error.message}
          </li>
        ))}
      </ul>
      {job.errors_truncated && (
        <p className="text-xs text-muted-foreground">
          {t("profile.importBookmarks.errorsTruncated")}
        </p>
      )}
    </div>
  );
}
