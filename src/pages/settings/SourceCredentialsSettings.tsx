import { type FormEvent, useState } from "react";
import { KeyRoundIcon, PlugZapIcon, Trash2Icon } from "lucide-react";

import {
  type SourceCredential,
  useDeleteSourceCredentialMutation,
  useErrorText,
  useSaveSourceCredentialMutation,
  useSourceCredentialsQuery,
} from "@/api";
import { useT } from "@/lib/i18n";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";
import { Label } from "@/ui/shadcn/label";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Spinner } from "@/ui/shadcn/spinner";

/**
 * Личные учётки источников. Ручная вставка Bearer/Cookie — временный путь:
 * вход по кнопке через расширение появится в отдельной фазе.
 */
export function SourceCredentialsSettings() {
  const t = useT();
  const query = useSourceCredentialsQuery();
  const errorText = useErrorText(query.error);

  if (query.isLoading) {
    return <Skeleton className="h-64 w-full" />;
  }

  if (query.isError) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t("settingsPage.sources.title")}</CardTitle>
          <CardDescription>
            {t("settingsPage.sources.description")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        </CardContent>
      </Card>
    );
  }

  const credentials = query.data?.credentials ?? [];

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t("settingsPage.sources.title")}</CardTitle>
          <CardDescription>
            {t("settingsPage.sources.description")}
          </CardDescription>
        </CardHeader>
      </Card>

      {credentials.map((credential) => (
        <CredentialCard key={credential.code} credential={credential} />
      ))}
    </div>
  );
}

function CredentialCard({ credential }: { credential: SourceCredential }) {
  const t = useT();
  const [bearer, setBearer] = useState("");
  const [cookie, setCookie] = useState("");
  const save = useSaveSourceCredentialMutation();
  const remove = useDeleteSourceCredentialMutation();
  const errorText = useErrorText(save.error ?? remove.error);

  const pending = save.isPending || remove.isPending;
  const canSave = !pending && (bearer.trim() !== "" || cookie.trim() !== "");
  const bearerId = `source-credential-bearer-${credential.code}`;
  const cookieId = `source-credential-cookie-${credential.code}`;

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!canSave) return;
    save.mutate(
      {
        code: credential.code,
        input: { bearer: bearer.trim() || null, cookie: cookie.trim() || null },
      },
      {
        onSuccess: () => {
          setBearer("");
          setCookie("");
        },
      },
    );
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div className="flex flex-col gap-1">
            <CardTitle className="flex items-center gap-2">
              <KeyRoundIcon aria-hidden className="size-4 text-muted-foreground" />
              {credential.name}
            </CardTitle>
            <CardDescription>
              {t("settingsPage.sources.cardDescription")}
            </CardDescription>
          </div>
          <Badge variant={credential.configured ? "secondary" : "outline"}>
            {credential.configured
              ? t("settingsPage.sources.connected")
              : t("settingsPage.sources.notConnected")}
          </Badge>
        </div>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        <form className="flex flex-col gap-3" onSubmit={handleSubmit}>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor={bearerId}>
                {t("settingsPage.sources.bearer")}
              </Label>
              <Input
                id={bearerId}
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={bearer}
                disabled={pending}
                placeholder={t("settingsPage.sources.bearerPlaceholder")}
                onChange={(event) => {
                  save.reset();
                  setBearer(event.target.value);
                }}
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor={cookieId}>
                {t("settingsPage.sources.cookie")}
              </Label>
              <Input
                id={cookieId}
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={cookie}
                disabled={pending}
                placeholder={t("settingsPage.sources.cookiePlaceholder")}
                onChange={(event) => {
                  save.reset();
                  setCookie(event.target.value);
                }}
              />
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Button type="submit" disabled={!canSave}>
              {save.isPending && <Spinner />}
              {t("settingsPage.sources.save")}
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={pending || !credential.configured}
              onClick={() => {
                save.reset();
                remove.mutate(credential.code);
              }}
            >
              {remove.isPending ? <Spinner /> : <Trash2Icon aria-hidden />}
              {t("settingsPage.sources.clear")}
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled
              title={t("settingsPage.sources.testHint")}
            >
              <PlugZapIcon aria-hidden />
              {t("settingsPage.sources.test")}
            </Button>
          </div>
        </form>

        {credential.configured && (
          <p className="text-xs text-muted-foreground">
            {t("settingsPage.sources.replaceHint")}
          </p>
        )}

        {credential.last_error && (
          <p role="alert" className="text-sm text-destructive">
            {credential.last_error === "auth_invalid"
              ? t("settingsPage.sources.authInvalid")
              : t("settingsPage.sources.lastError", {
                  code: credential.last_error,
                })}
          </p>
        )}

        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}

        <p className="text-xs text-muted-foreground">
          {t("settingsPage.sources.manualHint")}
        </p>
      </CardContent>
    </Card>
  );
}
