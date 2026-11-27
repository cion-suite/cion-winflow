import { type FormEvent, useState } from "react";
import { KeyRoundIcon, LogOutIcon, RefreshCwIcon } from "lucide-react";

import {
  type SourceCredential,
  useDeleteSourceCredentialMutation,
  useErrorText,
  useLoginSourceCredentialMutation,
  useSourceCredentialsQuery,
  useVerifySourceCredentialMutation,
} from "@/api";
import { useLocale, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { SourceIcon } from "@/ui/SourceIcon";
import { Button } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";
import { Field, FieldGroup, FieldLabel } from "@/ui/shadcn/field";
import { Input } from "@/ui/shadcn/input";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/ui/shadcn/item";
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "@/ui/shadcn/popover";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { Spinner } from "@/ui/shadcn/spinner";

/**
 * Личные учётки источников: одна строка на источник. Вход по логину и
 * паролю скрыт в поповере, пароль не сохраняется; статус показывает
 * свежесть проверки.
 */
export function SourceCredentialsSettings() {
  const t = useT();
  const query = useSourceCredentialsQuery();
  const errorText = useErrorText(query.error);
  const credentials = query.data?.credentials ?? [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("settingsPage.sources.title")}</CardTitle>
        <CardDescription>
          {t("settingsPage.sources.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {query.isLoading && <Skeleton className="h-14 w-full" />}
        {query.isError && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}
        {!query.isLoading && !query.isError && (
          <ItemGroup className="gap-0 divide-y overflow-hidden rounded-xl border">
            {credentials.map((credential) => (
              <CredentialRow key={credential.code} credential={credential} />
            ))}
          </ItemGroup>
        )}
      </CardContent>
    </Card>
  );
}

type StatusTone = "ok" | "warning" | "error" | "idle";

function CredentialRow({ credential }: { credential: SourceCredential }) {
  const t = useT();
  const locale = useLocale();
  const [loginOpen, setLoginOpen] = useState(false);
  const [loginName, setLoginName] = useState("");
  const [password, setPassword] = useState("");
  const signIn = useLoginSourceCredentialMutation();
  const verify = useVerifySourceCredentialMutation();
  const remove = useDeleteSourceCredentialMutation();
  const signInError = useErrorText(signIn.error);
  const verifyError = useErrorText(verify.error);
  const removeError = useErrorText(remove.error);

  const pending = signIn.isPending || verify.isPending || remove.isPending;
  const canSignIn = !pending && loginName.trim() !== "" && password !== "";
  const loginId = `source-credential-login-${credential.code}`;
  const passwordId = `source-credential-password-${credential.code}`;

  const tone = statusTone(credential);
  const label = statusLabel(credential, t, locale);

  function handleLogin(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!canSignIn) return;
    signIn.mutate(
      {
        code: credential.code,
        input: { login: loginName.trim(), password },
      },
      {
        onSuccess: () => {
          setLoginOpen(false);
          setLoginName("");
          setPassword("");
        },
      },
    );
  }

  return (
    <Item size="sm" className="rounded-none">
      <ItemMedia className="size-8 rounded-md bg-muted p-1">
        <SourceIcon code={credential.code} />
      </ItemMedia>

      <ItemContent>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <ItemTitle>{credential.name}</ItemTitle>
          <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            <StatusDot tone={tone} />
            {label}
          </span>
        </div>
        {verifyError && (
          <p role="alert" className="text-xs text-destructive">
            {verifyError}
          </p>
        )}
        {removeError && (
          <p role="alert" className="text-xs text-destructive">
            {removeError}
          </p>
        )}
      </ItemContent>

      <ItemActions>
        {credential.login_supported && (
          <Popover
            open={loginOpen}
            onOpenChange={(open) => {
              if (open) signIn.reset();
              setLoginOpen(open);
            }}
          >
            <PopoverTrigger
              render={
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("settingsPage.sources.account")}
                  title={t("settingsPage.sources.account")}
                />
              }
            >
              <KeyRoundIcon aria-hidden />
            </PopoverTrigger>
            <PopoverContent
              align="end"
              className="w-80 max-w-[calc(100vw-2rem)]"
            >
              <PopoverHeader>
                <PopoverTitle>
                  {t("settingsPage.sources.signInTitle", {
                    source: credential.name,
                  })}
                </PopoverTitle>
                <PopoverDescription>
                  {t("settingsPage.sources.signInDescription")}
                </PopoverDescription>
              </PopoverHeader>

              <form className="flex flex-col gap-4" onSubmit={handleLogin}>
                <FieldGroup className="gap-3">
                  <Field>
                    <FieldLabel htmlFor={loginId}>
                      {t("settingsPage.sources.login")}
                    </FieldLabel>
                    <Input
                      id={loginId}
                      autoComplete="off"
                      spellCheck={false}
                      value={loginName}
                      disabled={pending}
                      placeholder={t("settingsPage.sources.loginPlaceholder")}
                      onChange={(event) => {
                        signIn.reset();
                        setLoginName(event.target.value);
                      }}
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={passwordId}>
                      {t("settingsPage.sources.password")}
                    </FieldLabel>
                    <Input
                      id={passwordId}
                      type="password"
                      autoComplete="off"
                      value={password}
                      disabled={pending}
                      placeholder={t(
                        "settingsPage.sources.passwordPlaceholder",
                      )}
                      onChange={(event) => {
                        signIn.reset();
                        setPassword(event.target.value);
                      }}
                    />
                  </Field>
                </FieldGroup>

                {signInError && (
                  <p role="alert" className="text-sm text-destructive">
                    {signInError}
                  </p>
                )}

                <Button type="submit" disabled={!canSignIn}>
                  {signIn.isPending && <Spinner />}
                  {t("settingsPage.sources.signIn")}
                </Button>
              </form>
            </PopoverContent>
          </Popover>
        )}

        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={!credential.configured || pending}
          aria-label={t("settingsPage.sources.verify")}
          title={t("settingsPage.sources.verifyHint")}
          onClick={() => verify.mutate(credential.code)}
        >
          {verify.isPending ? (
            <Spinner />
          ) : (
            <RefreshCwIcon data-icon="inline-start" />
          )}
          <span className="hidden sm:inline">
            {t("settingsPage.sources.verify")}
          </span>
        </Button>

        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          disabled={!credential.configured || pending}
          aria-label={t("settingsPage.sources.disconnect")}
          title={t("settingsPage.sources.disconnect")}
          onClick={() => remove.mutate(credential.code)}
        >
          {remove.isPending ? <Spinner /> : <LogOutIcon aria-hidden />}
        </Button>
      </ItemActions>
    </Item>
  );
}

function statusTone(credential: SourceCredential): StatusTone {
  if (credential.last_error !== null) return "error";
  if (!credential.configured) return "idle";
  const verifiedAt = credential.last_verified_at;
  if (verifiedAt === null) return "warning";
  const fresh = Date.now() / 1000 - verifiedAt < 24 * 60 * 60;
  return fresh ? "ok" : "warning";
}

function statusLabel(
  credential: SourceCredential,
  t: ReturnType<typeof useT>,
  locale: string,
): string {
  if (credential.last_error !== null) {
    return credential.last_error === "auth_invalid"
      ? t("settingsPage.sources.status.rejected")
      : t("settingsPage.sources.lastError", { code: credential.last_error });
  }
  if (!credential.configured) {
    return t("settingsPage.sources.status.notConnected");
  }
  const verifiedAt = credential.last_verified_at;
  if (verifiedAt === null) {
    return t("settingsPage.sources.status.notChecked");
  }
  return t("settingsPage.sources.status.verified", {
    when: relativeTime(verifiedAt, locale),
  });
}

/** «5 минут назад» без лишней зависимости: Intl знает локали. */
function relativeTime(unix: number, locale: string): string {
  const formatter = new Intl.RelativeTimeFormat(
    locale === "ru" ? "ru-RU" : "en-GB",
    { numeric: "auto", style: "short" },
  );
  const diff = unix - Math.floor(Date.now() / 1000);
  const abs = Math.abs(diff);
  if (abs < 60) return formatter.format(0, "second");
  if (abs < 60 * 60) return formatter.format(Math.round(diff / 60), "minute");
  if (abs < 24 * 60 * 60) {
    return formatter.format(Math.round(diff / 3600), "hour");
  }
  return formatter.format(Math.round(diff / 86400), "day");
}

function StatusDot({ tone }: { tone: StatusTone }) {
  if (tone === "ok") {
    return (
      <span className="relative flex size-2">
        <span className="absolute inline-flex size-full animate-ping rounded-full bg-success opacity-75" />
        <span className="relative inline-flex size-2 rounded-full bg-success" />
      </span>
    );
  }
  return (
    <span
      className={cn(
        "size-2 rounded-full",
        tone === "error"
          ? "bg-destructive"
          : tone === "warning"
            ? "bg-warning"
            : "bg-muted-foreground",
      )}
    />
  );
}
