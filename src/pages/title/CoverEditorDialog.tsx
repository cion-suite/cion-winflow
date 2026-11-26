import { ImagesIcon, RefreshCwIcon, Trash2Icon, UploadIcon } from "lucide-react";
import { useState } from "react";

import {
  titleCoverUrl,
  useDeleteCoverMutation,
  useErrorText,
  useFetchSourceCoversMutation,
  useUploadCoverMutation,
  type Title,
  type TitleSource,
} from "@/api";
import { COVER_SIZES, type CoverSize } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { CoverImage } from "@/ui/CoverImage";
import { Badge } from "@/ui/shadcn/badge";
import { Button, buttonVariants } from "@/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui/shadcn/dialog";
import { Spinner } from "@/ui/shadcn/spinner";

type CoverEditorDialogProps = {
  title: Title;
  /** Привязанные источники: кнопка «Получить из источника» работает только при них. */
  sources: TitleSource[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/**
 * Обложки хранятся по размерам: low — поиск и добавление, mid — библиотека,
 * high — страница тайтла. До трёх файлов; отсутствующий размер сервер
 * отдаёт из ближайшего доступного.
 */
export function CoverEditorDialog({
  title,
  sources,
  open,
  onOpenChange,
}: CoverEditorDialogProps) {
  const t = useT();
  const upload = useUploadCoverMutation(title.id);
  const remove = useDeleteCoverMutation(title.id);
  const fromSource = useFetchSourceCoversMutation(title.id);
  const [confirming, setConfirming] = useState(false);
  const errorText = useErrorText(upload.error ?? remove.error ?? fromSource.error);
  const pending = upload.isPending || remove.isPending || fromSource.isPending;
  const pendingSize = upload.isPending
    ? upload.variables?.size
    : remove.isPending
      ? remove.variables
      : null;

  function handleOpenChange(next: boolean): void {
    if (!next) {
      upload.reset();
      remove.reset();
      fromSource.reset();
      setConfirming(false);
    }
    onOpenChange(next);
  }

  function resetResults(): void {
    upload.reset();
    remove.reset();
    fromSource.reset();
  }

  function handleFile(size: CoverSize, file: File | undefined): void {
    if (!file) return;
    resetResults();
    setConfirming(false);
    upload.mutate({ size, file });
  }

  function handleRemove(size: CoverSize): void {
    resetResults();
    setConfirming(false);
    remove.mutate(size);
  }

  function fetchFromSource(replaceManual: boolean): void {
    resetResults();
    setConfirming(false);
    fromSource.mutate({ replaceManual });
  }

  function handleFetchFromSource(): void {
    resetResults();
    // Ручной размер заменяется только после явного согласия.
    if (title.manual_cover_sizes.length > 0) {
      setConfirming(true);
      return;
    }
    fetchFromSource(false);
  }

  const sizesText = (sizes: CoverSize[]): string =>
    sizes.map((size) => t(`title.coverSizes.${size}`)).join(", ");

  const sourceName = (code: string): string =>
    sources.find((source) => source.code === code)?.name ?? code;

  const result = fromSource.data;
  const resultMessages: string[] = [];
  if (result) {
    if (result.imported.length > 0) {
      resultMessages.push(
        t("title.coverFromSourceUpdated", {
          sources: result.sources.map(sourceName).join(", "),
          sizes: sizesText(result.imported),
        }),
      );
    }
    if (result.skipped_manual.length > 0) {
      resultMessages.push(
        t("title.coverFromSourceKeptManual", {
          sizes: sizesText(result.skipped_manual),
        }),
      );
    }
    if (
      result.imported.length === 0 &&
      result.skipped_manual.length === 0 &&
      result.errors.length === 0
    ) {
      resultMessages.push(t("title.coverFromSourceEmpty"));
    }
    // Ошибки не теряются при частичном успехе: источник A мог упасть,
    // а источник B — отдать обложку.
    for (const error of result.errors) {
      resultMessages.push(
        t("title.coverFromSourceFailed", {
          source: sourceName(error.source_code),
          message: error.message,
        }),
      );
    }
  }

  const enabledSources = sources.filter((source) => source.enabled);

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ImagesIcon className="size-4" aria-hidden />
            {t("title.covers")}
          </DialogTitle>
          <DialogDescription>{t("title.coverEditorDescription")}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-2">
          {COVER_SIZES.map((size) => {
            const exists = title.cover_sizes.includes(size);
            const uploading = upload.isPending && pendingSize === size;
            const removing = remove.isPending && pendingSize === size;

            return (
              <div
                key={size}
                className="flex items-center gap-3 rounded-xl border p-3"
              >
                <CoverImage
                  src={exists ? titleCoverUrl(title, size) : null}
                  alt={`${title.name} — ${t(`title.coverSizes.${size}`)}`}
                  className="aspect-2/3 w-16 shrink-0 rounded-md ring-1 ring-foreground/10"
                />

                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium">
                      {t(`title.coverSizes.${size}`)}
                    </span>
                    <Badge variant={exists ? "secondary" : "outline"}>
                      {exists ? t("title.coverPresent") : t("title.coverMissing")}
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t(`title.coverSizeHints.${size}`)}
                  </p>
                </div>

                <div className="flex shrink-0 flex-col gap-1">
                  <label
                    className={cn(
                      buttonVariants({ variant: "outline", size: "sm" }),
                      "cursor-pointer",
                      // Фокус получает скрытый input, поэтому ring рисуем
                      // на обёртке через focus-within, иначе его не видно.
                      "focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50",
                      pending && "pointer-events-none opacity-50",
                    )}
                  >
                    {uploading ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <UploadIcon data-icon="inline-start" />
                    )}
                    {exists ? t("title.replaceCover") : t("title.uploadCover")}
                    <input
                      type="file"
                      className="sr-only"
                      accept="image/jpeg,image/png,image/webp,image/gif"
                      aria-label={`${t("title.uploadCover")}: ${t(`title.coverSizes.${size}`)}`}
                      disabled={pending}
                      onChange={(event) => {
                        handleFile(size, event.target.files?.[0]);
                        event.target.value = "";
                      }}
                    />
                  </label>
                  {exists && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      aria-label={`${t("title.removeCover")}: ${t(`title.coverSizes.${size}`)}`}
                      disabled={pending}
                      onClick={() => handleRemove(size)}
                    >
                      {removing ? (
                        <Spinner data-icon="inline-start" />
                      ) : (
                        <Trash2Icon data-icon="inline-start" />
                      )}
                      {t("title.removeCover")}
                    </Button>
                  )}
                </div>
              </div>
            );
          })}

          <p className="text-xs text-muted-foreground">{t("title.coverHint")}</p>

          {sources.length > 0 && (
            <div className="flex flex-col gap-2 rounded-xl border p-3">
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={pending || enabledSources.length === 0}
                  onClick={handleFetchFromSource}
                >
                  {fromSource.isPending ? (
                    <Spinner data-icon="inline-start" />
                  ) : (
                    <RefreshCwIcon data-icon="inline-start" />
                  )}
                  {t("title.coverFromSource")}
                </Button>
                <p className="text-xs text-muted-foreground">
                  {enabledSources.length > 0
                    ? t("title.coverFromSourceHint")
                    : t("title.coverFromSourceNoEnabled")}
                </p>
              </div>

              {confirming && (
                <div
                  role="alert"
                  className="flex flex-col gap-2 rounded-lg bg-muted/50 p-2"
                >
                  <p className="text-sm">
                    {t("title.coverFromSourceConfirm", {
                      sizes: sizesText(title.manual_cover_sizes),
                    })}
                  </p>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      type="button"
                      size="sm"
                      disabled={pending}
                      onClick={() => fetchFromSource(true)}
                    >
                      {t("title.coverFromSourceReplace")}
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={pending}
                      onClick={() => fetchFromSource(false)}
                    >
                      {t("title.coverFromSourceKeepManual")}
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      disabled={pending}
                      onClick={() => setConfirming(false)}
                    >
                      {t("title.cancel")}
                    </Button>
                  </div>
                </div>
              )}

              {resultMessages.length > 0 && !confirming && (
                <div role="status" className="flex flex-col gap-1">
                  {resultMessages.map((message) => (
                    <p key={message} className="text-sm text-muted-foreground">
                      {message}
                    </p>
                  ))}
                </div>
              )}
            </div>
          )}

          {errorText && (
            <p role="alert" className="text-sm text-destructive">
              {errorText}
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
