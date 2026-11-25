import { ImagesIcon, Trash2Icon, UploadIcon } from "lucide-react";

import {
  titleCoverUrl,
  useDeleteCoverMutation,
  useErrorText,
  useUploadCoverMutation,
  type Title,
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
  open,
  onOpenChange,
}: CoverEditorDialogProps) {
  const t = useT();
  const upload = useUploadCoverMutation(title.id);
  const remove = useDeleteCoverMutation(title.id);
  const errorText = useErrorText(upload.error ?? remove.error);
  const pending = upload.isPending || remove.isPending;
  const pendingSize = upload.isPending
    ? upload.variables?.size
    : remove.isPending
      ? remove.variables
      : null;

  function handleOpenChange(next: boolean): void {
    if (!next) {
      upload.reset();
      remove.reset();
    }
    onOpenChange(next);
  }

  function handleFile(size: CoverSize, file: File | undefined): void {
    if (!file) return;
    upload.reset();
    remove.reset();
    upload.mutate({ size, file });
  }

  function handleRemove(size: CoverSize): void {
    upload.reset();
    remove.reset();
    remove.mutate(size);
  }

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
