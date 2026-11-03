import { version } from "../../../package.json";
import { useT } from "@/lib/i18n";
import { PageHeader } from "@/ui/PageHeader";
import { Card, CardContent, CardHeader, CardTitle } from "@/ui/shadcn/card";
import { RELEASES } from "./release-notes";

export function ChangelogPage() {
  const t = useT();

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <PageHeader
        title={t("nav.changelog")}
        description={t("changelog.description")}
        actions={
          <span className="rounded-md border px-2 py-0.5 font-mono text-xs text-muted-foreground">
            v{version}
          </span>
        }
      />
      {RELEASES.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("changelog.emptyDescription")}</p>
      ) : (
        RELEASES.map((entry) => (
          <Card key={entry.version}>
            <CardHeader>
              <CardTitle className="font-mono">v{entry.version}</CardTitle>
            </CardHeader>
            <CardContent>
              <ul className="list-disc space-y-1 pl-5 text-sm">
                {entry.bullets.map((bullet) => (
                  <li key={bullet}>{bullet}</li>
                ))}
              </ul>
            </CardContent>
          </Card>
        ))
      )}
    </div>
  );
}
