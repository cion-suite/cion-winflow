import { type FormEvent, type ReactNode, useState } from "react";
import { useParams } from "react-router";

import {
  formValuesToTitleInput,
  titleCoverUrl,
  titleToFormValues,
  useAuth,
  useErrorText,
  useTitleQuery,
  useUpdateTitleMutation,
  useUploadCoverMutation,
} from "@/api";
import type { TitleFormValues } from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { CoverImage } from "@/ui/CoverImage";
import { PageHeader } from "@/ui/PageHeader";
import { TitleForm } from "@/ui/TitleForm";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";
import { Skeleton } from "@/ui/shadcn/skeleton";
import { LibraryPanel } from "./LibraryPanel";

function MetaRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[160px_1fr] sm:gap-3">
      <span className="text-sm text-muted-foreground">{label}</span>
      <div className="text-sm">{children}</div>
    </div>
  );
}

function CoverEditor({ titleId }: { titleId: number }) {
  const t = useT();
  const [file, setFile] = useState<File | null>(null);
  const upload = useUploadCoverMutation(titleId);
  const errorText = useErrorText(upload.error);

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!file || upload.isPending) return;
    const form = event.currentTarget;
    upload.mutate(file, {
      onSuccess: () => {
        setFile(null);
        form.reset();
      },
    });
  }

  return (
    <form className="flex flex-col gap-2" onSubmit={handleSubmit}>
      <Input
        type="file"
        name="file"
        aria-label={t("title.uploadCover")}
        accept="image/jpeg,image/png,image/webp,image/gif"
        onChange={(event) => {
          upload.reset();
          setFile(event.target.files?.[0] ?? null);
        }}
      />
      <Button type="submit" variant="outline" disabled={!file || upload.isPending}>
        {t("title.uploadCover")}
      </Button>
      <p className="text-xs text-muted-foreground">{t("title.coverHint")}</p>
      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}
    </form>
  );
}

export function TitlePage() {
  const t = useT();
  const { id } = useParams();
  const { user } = useAuth();
  const parsed = Number(id);
  const titleId = Number.isInteger(parsed) && parsed > 0 ? parsed : null;

  const query = useTitleQuery(titleId);
  const update = useUpdateTitleMutation(titleId ?? 0);
  const [editing, setEditing] = useState(false);

  const queryError = useErrorText(query.error);
  const updateError = useErrorText(update.error);
  const isAdmin = user?.role === "admin";

  if (titleId === null) {
    return (
      <Card>
        <CardContent>
          <p role="alert" className="text-sm text-destructive">
            {t("errors.titleNotFound")}
          </p>
        </CardContent>
      </Card>
    );
  }

  if (query.isPending) {
    return (
      <div className="mx-auto grid w-full max-w-5xl gap-6 md:grid-cols-[240px_1fr]">
        <Skeleton className="aspect-2/3 w-full rounded-xl" />
        <div className="flex flex-col gap-3">
          <Skeleton className="h-8 w-1/2" />
          <Skeleton className="h-4 w-1/3" />
          <Skeleton className="h-32 w-full rounded-xl" />
        </div>
      </div>
    );
  }

  if (query.isError || !query.data) {
    return (
      <Card>
        <CardContent className="flex flex-col items-start gap-2">
          <p role="alert" className="text-sm text-destructive">
            {queryError}
          </p>
          <Button type="button" variant="outline" onClick={() => void query.refetch()}>
            {t("retry")}
          </Button>
        </CardContent>
      </Card>
    );
  }

  const { title, library } = query.data;
  const meta = [
    t(`catalog.kinds.${title.kind}`),
    title.year === null ? null : String(title.year),
    title.status === null ? null : t(`title.statuses.${title.status}`),
  ]
    .filter((part) => part !== null)
    .join(" · ");

  function handleUpdate(values: TitleFormValues): void {
    update.mutate(formValuesToTitleInput(values), {
      onSuccess: () => setEditing(false),
    });
  }

  function toggleEditing(): void {
    update.reset();
    setEditing((value) => !value);
  }

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
      <PageHeader
        title={title.name}
        description={meta}
        actions={
          isAdmin && (
            <Button type="button" variant="outline" onClick={toggleEditing}>
              {t("title.edit")}
            </Button>
          )
        }
      />

      <div className="grid gap-6 md:grid-cols-[240px_1fr]">
        <div className="flex flex-col gap-3">
          <CoverImage
            src={title.has_cover ? titleCoverUrl(title, "full") : null}
            alt={title.name}
            loading="eager"
            className="aspect-2/3 w-full rounded-xl ring-1 ring-foreground/10"
          />
          {isAdmin && <CoverEditor titleId={titleId} />}
        </div>

        <div className="flex min-w-0 flex-col gap-4">
          {editing && isAdmin && (
            <Card>
              <CardHeader>
                <CardTitle>{t("title.editTitle")}</CardTitle>
              </CardHeader>
              <CardContent>
                <TitleForm
                  initial={titleToFormValues(title)}
                  submitLabel={t("title.save")}
                  isPending={update.isPending}
                  errorText={updateError}
                  onSubmit={handleUpdate}
                  onCancel={() => {
                    update.reset();
                    setEditing(false);
                  }}
                />
              </CardContent>
            </Card>
          )}

          <Card>
            <CardHeader>
              <CardTitle>{t("library.title")}</CardTitle>
            </CardHeader>
            <CardContent>
              <LibraryPanel
                key={`${titleId}:${
                  library === null
                    ? "not-in-library"
                    : `${library.status}:${library.score ?? ""}`
                }`}
                titleId={titleId}
                entry={library}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>{t("title.about")}</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-3">
              {title.alt_names.length > 0 && (
                <MetaRow label={t("title.altNames")}>{title.alt_names.join(", ")}</MetaRow>
              )}
              {title.authors.length > 0 && (
                <MetaRow label={t("title.authors")}>{title.authors.join(", ")}</MetaRow>
              )}
              {title.genres.length > 0 && (
                <MetaRow label={t("title.genres")}>{title.genres.join(", ")}</MetaRow>
              )}
              {title.lang !== null && (
                <MetaRow label={t("title.lang")}>{title.lang}</MetaRow>
              )}
              {title.rating !== null && (
                <MetaRow label={t("title.rating")}>{title.rating.toFixed(1)}</MetaRow>
              )}
              <MetaRow label={t("title.description")}>
                {title.description === null ? (
                  <span className="text-muted-foreground">{t("title.noDescription")}</span>
                ) : (
                  <p className="whitespace-pre-line">{title.description}</p>
                )}
              </MetaRow>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>{t("title.chapters")}</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground">{t("title.chaptersEmpty")}</p>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
