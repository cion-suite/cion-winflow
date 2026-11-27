import { DownloadIcon, UploadIcon } from "lucide-react";

import {
  useBookmarkImportQuery,
  useErrorText,
  useExportBookmarksMutation,
} from "@/api";
import { useT } from "@/lib/i18n";
import { SourceIcon } from "@/ui/SourceIcon";
import { Button } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";
import { Spinner } from "@/ui/shadcn/spinner";

type ImportExportSettingsProps = {
  onOpenImport: () => void;
};

/** Закладки Remanga: импорт из JSON и экспорт библиотеки в том же формате. */
export function ImportExportSettings({
  onOpenImport,
}: ImportExportSettingsProps) {
  const t = useT();
  const latest = useBookmarkImportQuery();
  const errorText = useErrorText(latest.error);
  const exportMutation = useExportBookmarksMutation();
  const exportError = useErrorText(exportMutation.error);
  const job = latest.data?.job ?? null;
  const running = job?.state === "running";

  function handleExport(): void {
    exportMutation.mutate(undefined, {
      onSuccess: (blob) => downloadBlob(blob, "bookmarks.json"),
    });
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-3">
          <div className="grid size-10 shrink-0 place-items-center rounded-lg bg-muted p-1.5">
            <SourceIcon code="remanga" className="size-6" />
          </div>
          <div className="flex min-w-0 flex-col gap-1">
            <CardTitle>{t("settingsPage.importExport.title")}</CardTitle>
            <CardDescription>
              {t("settingsPage.importExport.description")}
            </CardDescription>
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" onClick={onOpenImport}>
            <DownloadIcon data-icon="inline-start" />
            {t("settingsPage.import.action")}
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={exportMutation.isPending}
            onClick={handleExport}
          >
            {exportMutation.isPending ? (
              <Spinner />
            ) : (
              <UploadIcon data-icon="inline-start" />
            )}
            {t("settingsPage.export.action")}
          </Button>
        </div>

        {running && job && (
          <p className="text-xs text-muted-foreground">
            {t("settingsPage.import.running", {
              processed: job.processed,
              total: job.total,
            })}
          </p>
        )}
        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}
        {exportError && (
          <p role="alert" className="text-sm text-destructive">
            {exportError}
          </p>
        )}

        <div className="flex flex-col gap-1 text-xs text-muted-foreground">
          <p>{t("settingsPage.import.reimportHint")}</p>
          <p>{t("settingsPage.export.hint")}</p>
        </div>
      </CardContent>
    </Card>
  );
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  // Некоторые браузеры читают Blob уже после клика.
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}
