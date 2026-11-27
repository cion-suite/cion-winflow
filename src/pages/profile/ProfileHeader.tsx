import {
  BookOpenIcon,
  CalendarIcon,
  CheckCheckIcon,
  LibraryBigIcon,
  type LucideIcon,
} from "lucide-react";

import { userAvatarUrl, type ProfileUser } from "@/api";
import { useLocale, useT } from "@/lib/i18n";
import { roleLabelKey } from "@/lib/nav";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@/ui/shadcn/avatar";
import { Badge } from "@/ui/shadcn/badge";
import {
  Card,
  CardContent,
} from "@/ui/shadcn/card";
import { Separator } from "@/ui/shadcn/separator";

type ProfileHeaderProps = {
  user: ProfileUser;
  readChapters: number;
  titles: number;
  completed: number;
};

/**
 * Компактная шапка профиля: аватар, ник, роль, дата регистрации и счётчики.
 * Редактирование аватара и импорт живут в настройках.
 */
export function ProfileHeader({
  user,
  readChapters,
  titles,
  completed,
}: ProfileHeaderProps) {
  const t = useT();

  return (
    <Card>
      <CardContent className="flex flex-col gap-5 sm:flex-row sm:items-center sm:gap-6">
        <ProfileAvatar user={user} />
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

function ProfileAvatar({ user }: { user: ProfileUser }) {
  const src = userAvatarUrl(
    user.id,
    user.avatar_updated_at,
    user.avatar_version,
  );
  const initial = user.username.trim().slice(0, 1).toUpperCase();

  return (
    <Avatar className="size-16">
      <AvatarImage src={src ?? undefined} alt="" />
      <AvatarFallback>{initial}</AvatarFallback>
    </Avatar>
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
