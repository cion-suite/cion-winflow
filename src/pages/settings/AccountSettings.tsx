import {
  type ChangeEvent,
  type FormEvent,
  useRef,
  useState,
} from "react";
import { CameraIcon, XIcon } from "lucide-react";

import {
  useChangePasswordMutation,
  useDeleteAvatarMutation,
  useErrorText,
  useUpdateUsernameMutation,
  useUploadAvatarMutation,
  userAvatarUrl,
  type User,
} from "@/api";
import { useT } from "@/lib/i18n";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@/ui/shadcn/avatar";
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
import { Separator } from "@/ui/shadcn/separator";
import { Spinner } from "@/ui/shadcn/spinner";

export function AccountSettings({ user }: { user: User }) {
  const t = useT();

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>{t("settingsPage.account.title")}</CardTitle>
          <CardDescription>
            {t("settingsPage.account.description")}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-6">
          <AvatarEditor user={user} />
          <Separator />
          <UsernameForm user={user} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settingsPage.password.title")}</CardTitle>
          <CardDescription>
            {t("settingsPage.password.description")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <PasswordForm />
        </CardContent>
      </Card>
    </div>
  );
}

function AvatarEditor({ user }: { user: User }) {
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  const upload = useUploadAvatarMutation();
  const remove = useDeleteAvatarMutation();
  const errorText = useErrorText(upload.error ?? remove.error);
  const src = userAvatarUrl(
    user.id,
    user.avatar_updated_at,
    user.avatar_version,
  );
  const initial = user.username.trim().slice(0, 1).toUpperCase();
  const pending = upload.isPending || remove.isPending;

  function handleFile(event: ChangeEvent<HTMLInputElement>): void {
    const file = event.target.files?.[0];
    // Иначе повторный выбор того же файла не вызовет onChange
    event.target.value = "";
    if (file) {
      // Ошибка предыдущего удаления не должна висеть над новой загрузкой
      remove.reset();
      upload.mutate(file);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="text-sm font-medium">
          {t("settingsPage.account.avatar")}
        </p>
        <p className="text-xs text-muted-foreground">
          {t("settingsPage.account.avatarHint")}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-4">
        <Avatar className="size-20">
          <AvatarImage src={src ?? undefined} alt="" />
          <AvatarFallback>{initial}</AvatarFallback>
        </Avatar>

        <div className="flex flex-wrap items-center gap-2">
          <input
            ref={inputRef}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            aria-label={t("profile.changeAvatar")}
            tabIndex={-1}
            className="sr-only"
            onChange={handleFile}
          />
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={() => inputRef.current?.click()}
          >
            {upload.isPending ? <Spinner /> : <CameraIcon aria-hidden />}
            {t("profile.changeAvatar")}
          </Button>
          {src !== null && (
            <Button
              type="button"
              variant="outline"
              disabled={pending}
              onClick={() => {
                upload.reset();
                remove.mutate();
              }}
            >
              {remove.isPending ? <Spinner /> : <XIcon aria-hidden />}
              {t("profile.removeAvatar")}
            </Button>
          )}
        </div>
      </div>

      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}
    </div>
  );
}

const USERNAME_PATTERN = /^[A-Za-z0-9_-]{3,32}$/;

function UsernameForm({ user }: { user: User }) {
  const t = useT();
  // Пока пользователь не начал править, поле следует за актуальным user
  // (ник мог измениться в другой вкладке).
  const [draft, setDraft] = useState<string | null>(null);
  const username = draft ?? user.username;
  const update = useUpdateUsernameMutation();
  const errorText = useErrorText(update.error);

  const trimmed = username.trim();
  const valid = USERNAME_PATTERN.test(trimmed);
  const changed = trimmed !== user.username;

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!valid || !changed || update.isPending) return;
    update.mutate(trimmed, {
      // Сервер возвращает нормализованный ник; убираем пробелы из поля.
      onSuccess: (data) => setDraft(data.user.username),
    });
  }

  return (
    <form className="flex flex-col gap-2" onSubmit={handleSubmit}>
      <Label htmlFor="settings-username">
        {t("settingsPage.account.username")}
      </Label>
      <Input
        id="settings-username"
        value={username}
        maxLength={32}
        autoComplete="username"
        disabled={update.isPending}
        onChange={(event) => {
          update.reset();
          setDraft(event.target.value);
        }}
      />
      <p className="text-xs text-muted-foreground">
        {t("settingsPage.account.usernameHint")}
      </p>
      {update.isSuccess && !update.isPending && (
        <p className="text-sm text-muted-foreground">
          {t("settingsPage.account.usernameSaved")}
        </p>
      )}
      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}
      <Button
        type="submit"
        className="self-start"
        disabled={!valid || !changed || update.isPending}
      >
        {update.isPending && <Spinner />}
        {t("settingsPage.account.saveUsername")}
      </Button>
    </form>
  );
}

const MIN_PASSWORD_LENGTH = 10;

function PasswordForm() {
  const t = useT();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const change = useChangePasswordMutation();
  const errorText = useErrorText(change.error);

  const tooShort =
    next.length > 0 && [...next].length < MIN_PASSWORD_LENGTH;
  const mismatch = confirm.length > 0 && next !== confirm;
  const valid =
    current.length > 0 &&
    [...next].length >= MIN_PASSWORD_LENGTH &&
    next === confirm;

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!valid || change.isPending) return;
    change.mutate(
      { current_password: current, new_password: next },
      {
        onSuccess: () => {
          setCurrent("");
          setNext("");
          setConfirm("");
        },
      },
    );
  }

  function resetAndSet(setter: (value: string) => void, value: string): void {
    change.reset();
    setter(value);
  }

  return (
    <form className="flex max-w-sm flex-col gap-3" onSubmit={handleSubmit}>
      <div className="flex flex-col gap-2">
        <Label htmlFor="settings-current-password">
          {t("settingsPage.password.current")}
        </Label>
        <Input
          id="settings-current-password"
          type="password"
          autoComplete="current-password"
          value={current}
          disabled={change.isPending}
          onChange={(event) =>
            resetAndSet(setCurrent, event.target.value)
          }
        />
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="settings-new-password">
          {t("settingsPage.password.new")}
        </Label>
        <Input
          id="settings-new-password"
          type="password"
          autoComplete="new-password"
          value={next}
          disabled={change.isPending}
          onChange={(event) => resetAndSet(setNext, event.target.value)}
        />
        {tooShort && (
          <p className="text-xs text-destructive">
            {t("settingsPage.password.tooShort")}
          </p>
        )}
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="settings-confirm-password">
          {t("settingsPage.password.confirm")}
        </Label>
        <Input
          id="settings-confirm-password"
          type="password"
          autoComplete="new-password"
          value={confirm}
          disabled={change.isPending}
          onChange={(event) =>
            resetAndSet(setConfirm, event.target.value)
          }
        />
        {mismatch && (
          <p className="text-xs text-destructive">
            {t("settingsPage.password.mismatch")}
          </p>
        )}
      </div>

      {change.isSuccess && !change.isPending && (
        <p className="text-sm text-muted-foreground">
          {t("settingsPage.password.saved")}
        </p>
      )}
      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}

      <Button
        type="submit"
        className="self-start"
        disabled={!valid || change.isPending}
      >
        {change.isPending && <Spinner />}
        {t("settingsPage.password.save")}
      </Button>
    </form>
  );
}
