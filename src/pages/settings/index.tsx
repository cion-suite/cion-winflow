import { useState } from "react";

import { useAuth } from "@/api";
import { useT } from "@/lib/i18n";
import { PageHeader } from "@/ui/PageHeader";
import { Skeleton } from "@/ui/shadcn/skeleton";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/ui/shadcn/tabs";
import { AccountSettings } from "./AccountSettings";
import { BookmarksImportDialog } from "./BookmarksImportDialog";
import { ImportExportSettings } from "./ImportExportSettings";
import { SourceCredentialsSettings } from "./SourceCredentialsSettings";

export function SettingsPage() {
  const t = useT();
  const { user } = useAuth();
  const [importOpen, setImportOpen] = useState(false);

  if (!user) {
    return (
      <div className="flex w-full flex-col gap-5">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  return (
    <div className="flex w-full flex-col gap-5">
      <PageHeader
        title={t("settingsPage.title")}
        description={t("settingsPage.description")}
      />

      <Tabs defaultValue="import-export" className="w-full">
        <TabsList variant="line" className="w-full justify-start">
          <TabsTrigger value="import-export" className="flex-none px-3">
            {t("settingsPage.tabs.importExport")}
          </TabsTrigger>
          <TabsTrigger value="sources" className="flex-none px-3">
            {t("settingsPage.tabs.sources")}
          </TabsTrigger>
          <TabsTrigger value="account" className="flex-none px-3">
            {t("settingsPage.tabs.account")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="import-export">
          <ImportExportSettings onOpenImport={() => setImportOpen(true)} />
        </TabsContent>
        <TabsContent value="sources">
          <SourceCredentialsSettings />
        </TabsContent>
        <TabsContent value="account">
          <AccountSettings user={user} />
        </TabsContent>
      </Tabs>

      {/* Диалог живёт вне вкладок: опрос фонового импорта не прекращается,
          когда пользователь переключается на «Аккаунт». */}
      <BookmarksImportDialog
        open={importOpen}
        onOpenChange={setImportOpen}
      />
    </div>
  );
}
