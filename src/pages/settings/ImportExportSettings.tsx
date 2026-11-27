import { DownloadIcon, UploadIcon } from "lucide-react";

import {
  useBookmarkImportQuery,
  useErrorText,
  useExportBookmarksMutation,
} from "@/api";
import { useT } from "@/lib/i18n";
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
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t("settingsPage.import.title")}</CardTitle>
          <CardDescription>
            {t("settingsPage.import.description")}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col items-start gap-3">
          <Button type="button" variant="outline" onClick={onOpenImport}>
            <UploadIcon aria-hidden />
            {t("settingsPage.import.action")}
          </Button>
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
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settingsPage.export.title")}</CardTitle>
          <CardDescription>
            {t("settingsPage.export.description")}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col items-start gap-3">
          <Button
            type="button"
            variant="outline"
            disabled={exportMutation.isPending}
            onClick={handleExport}
          >
            {exportMutation.isPending ? (
              <Spinner />
            ) : (
              <DownloadIcon aria-hidden />
            )}
            {t("settingsPage.export.action")}
          </Button>
          {exportError && (
            <p role="alert" className="text-sm text-destructive">
              {exportError}
            </p>
          )}
          <p className="text-xs text-muted-foreground">
            {t("settingsPage.export.hint")}
          </p>
        </CardContent>
      </Card>
    </div>
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
