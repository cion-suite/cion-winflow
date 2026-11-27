import { type ChangeEvent, useRef } from "react";
import {
  BookOpenIcon,
  CalendarIcon,
  CameraIcon,
  CheckCheckIcon,
  LibraryBigIcon,
  XIcon,
  type LucideIcon,
} from "lucide-react";

import {
  useDeleteAvatarMutation,
  useErrorText,
  useUploadAvatarMutation,
  userAvatarUrl,
  type ProfileUser,
} from "@/api";
import { useLocale, useT } from "@/lib/i18n";
import { roleLabelKey } from "@/lib/nav";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@/ui/shadcn/avatar";
import { Badge } from "@/ui/shadcn/badge";
import { Button } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
} from "@/ui/shadcn/card";
import { Separator } from "@/ui/shadcn/separator";
import { Spinner } from "@/ui/shadcn/spinner";

type ProfileHeaderProps = {
  user: ProfileUser;
  readChapters: number;
  titles: number;
  completed: number;
  isOwn: boolean;
};

/** Компактная шапка профиля: аватар, ник, роль, дата регистрации и счётчики. */
export function ProfileHeader({
  user,
  readChapters,
  titles,
  completed,
  isOwn,
}: ProfileHeaderProps) {
  const t = useT();

  return (
    <Card>
      <CardContent className="flex flex-col gap-5 sm:flex-row sm:items-center sm:gap-6">
        <ProfileAvatar user={user} isOwn={isOwn} />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h1 className="min-w-0 truncate text-xl font-semibold tracking-tight">
              {user.username}
            </h1>
            <Badge variant="secondary">{t(roleLabelKey(user.role))}</Badge>
          </div>
          <ProfileMeta user={user} />
        </div>
        <Separator orientation="vertical" className="hidden h-12 sm:block" />
        <div className="grid grid-cols-3 gap-5 sm:gap-7">
          <Stat icon={BookOpenIcon} value={readChapters} labelKey="profile.stats.readChapters" />
          <Stat icon={LibraryBigIcon} value={titles} labelKey="profile.stats.titles" />
          <Stat icon={CheckCheckIcon} value={completed} labelKey="profile.stats.completed" />
        </div>
      </CardContent>
    </Card>
  );
}

function ProfileMeta({ user }: { user: ProfileUser }) {
  const t = useT();
  const locale = useLocale();
  const joined = new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(
    new Date(user.created_at * 1000),
  );

  return (
    <p className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
      <CalendarIcon className="size-3.5" aria-hidden />
      <span>{t("profile.joined", { date: joined })}</span>
      <span aria-hidden>·</span>
      <span className="tabular-nums">{t("profile.id", { id: user.id })}</span>
    </p>
  );
}

function ProfileAvatar({ user, isOwn }: { user: ProfileUser; isOwn: boolean }) {
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
    <div className="flex flex-col items-center gap-1.5">
      <div className="relative">
        <Avatar className="size-16">
          <AvatarImage src={src ?? undefined} alt="" />
          <AvatarFallback>{initial}</AvatarFallback>
        </Avatar>

        {isOwn && (
          <>
            <input
              ref={inputRef}
              type="file"
              accept="image/png,image/jpeg,image/webp,image/gif"
              className="sr-only"
              onChange={handleFile}
            />
            <Button
              type="button"
              variant="secondary"
              size="icon-sm"
              className="absolute -right-1 -bottom-1 rounded-full"
              aria-label={t("profile.changeAvatar")}
              title={t("profile.changeAvatar")}
              disabled={pending}
              onClick={() => inputRef.current?.click()}
            >
              {upload.isPending ? <Spinner /> : <CameraIcon />}
            </Button>
            {src !== null && (
              <Button
                type="button"
                variant="secondary"
                size="icon-sm"
                className="absolute -top-1 -right-1 rounded-full"
                aria-label={t("profile.removeAvatar")}
                title={t("profile.removeAvatar")}
                disabled={pending}
                onClick={() => {
                  upload.reset();
                  remove.mutate();
                }}
              >
                {remove.isPending ? <Spinner /> : <XIcon />}
              </Button>
            )}
          </>
        )}
      </div>

      {errorText !== null && (
        <p role="alert" className="max-w-40 text-center text-xs text-destructive">
          {errorText}
        </p>
      )}
    </div>
  );
}

function Stat({
  icon: Icon,
  value,
  labelKey,
}: {
  icon: LucideIcon;
  value: number;
  labelKey: string;
}) {
  const t = useT();

  return (
    <div className="flex flex-col items-center gap-0.5 text-center">
      <span className="inline-flex items-center gap-1 text-lg leading-none font-semibold tabular-nums">
        <Icon className="size-4 text-muted-foreground" aria-hidden />
        {value}
      </span>
      <span className="text-[11px] leading-tight text-muted-foreground">
        {t(labelKey)}
      </span>
    </div>
  );
}
