import { type FormEvent, type ReactNode, useState } from "react";

import {
  MEDIA_KINDS,
  TITLE_STATUSES,
  type TitleFormValues,
} from "@/lib/catalog";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/ui/shadcn/button";
import { Input } from "@/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/ui/shadcn/select";
import { Textarea } from "@/ui/Textarea";

type TitleFormProps = {
  initial: TitleFormValues;
  submitLabel: string;
  isPending: boolean;
  errorText: string | null;
  onSubmit: (values: TitleFormValues) => void;
  onCancel?: () => void;
};

function Field({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <label className={cn("flex flex-col gap-1.5", className)}>
      <span className="text-sm font-medium">{label}</span>
      {children}
    </label>
  );
}

/** Форма тайтла без знания API: значениями и отправкой управляет экран. */
export function TitleForm({
  initial,
  submitLabel,
  isPending,
  errorText,
  onSubmit,
  onCancel,
}: TitleFormProps) {
  const t = useT();
  const [values, setValues] = useState<TitleFormValues>(initial);

  const kindOptions = MEDIA_KINDS.map((kind) => ({
    value: kind,
    label: t(`catalog.kinds.${kind}`),
  }));
  const statusOptions = [
    { value: null, label: t("title.unset") },
    ...TITLE_STATUSES.map((status) => ({
      value: status,
      label: t(`title.statuses.${status}`),
    })),
  ];

  function update<K extends keyof TitleFormValues>(
    key: K,
    value: TitleFormValues[K],
  ): void {
    setValues((current) => ({ ...current, [key]: value }));
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (isPending) return;
    onSubmit(values);
  }

  return (
    <form className="flex flex-col gap-3" onSubmit={handleSubmit}>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label={t("title.name")} className="sm:col-span-2">
          <Input
            name="name"
            required
            maxLength={300}
            value={values.name}
            onChange={(event) => update("name", event.target.value)}
          />
        </Field>

        <Field label={t("title.kind")}>
          <Select
            items={kindOptions}
            name="kind"
            value={values.kind}
            onValueChange={(next) => {
              if (next !== null) update("kind", next);
            }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {kindOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <Field label={t("title.status")}>
          <Select
            items={statusOptions}
            name="status"
            value={values.status === "" ? null : values.status}
            onValueChange={(next) => update("status", next ?? "")}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {statusOptions.map((option) => (
                <SelectItem key={option.value ?? "unset"} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <Field label={t("title.altNames")}>
          <Input
            name="altNames"
            placeholder={t("title.listHint")}
            value={values.altNames}
            onChange={(event) => update("altNames", event.target.value)}
          />
        </Field>

        <Field label={t("title.authors")}>
          <Input
            name="authors"
            placeholder={t("title.listHint")}
            value={values.authors}
            onChange={(event) => update("authors", event.target.value)}
          />
        </Field>

        <Field label={t("title.genres")}>
          <Input
            name="genres"
            placeholder={t("title.listHint")}
            value={values.genres}
            onChange={(event) => update("genres", event.target.value)}
          />
        </Field>

        <Field label={t("title.year")}>
          <Input
            name="year"
            type="number"
            min={1800}
            max={2200}
            value={values.year}
            onChange={(event) => update("year", event.target.value)}
          />
        </Field>

        <Field label={t("title.lang")}>
          <Input
            name="lang"
            placeholder="ru"
            maxLength={8}
            value={values.lang}
            onChange={(event) => update("lang", event.target.value)}
          />
        </Field>

        <Field label={t("title.rating")}>
          <Input
            name="rating"
            type="number"
            min={0}
            max={10}
            step={0.1}
            value={values.rating}
            onChange={(event) => update("rating", event.target.value)}
          />
        </Field>

        <Field label={t("title.description")} className="sm:col-span-2">
          <Textarea
            name="description"
            value={values.description}
            onChange={(event) => update("description", event.target.value)}
          />
        </Field>
      </div>

      {errorText && (
        <p role="alert" className="text-sm text-destructive">
          {errorText}
        </p>
      )}

      <div className="flex items-center gap-2">
        <Button type="submit" disabled={isPending}>
          {submitLabel}
        </Button>
        {onCancel && (
          <Button type="button" variant="outline" onClick={onCancel} disabled={isPending}>
            {t("title.cancel")}
          </Button>
        )}
      </div>
    </form>
  );
}
