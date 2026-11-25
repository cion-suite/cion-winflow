import { type FormEvent, useMemo, useState } from "react";

import {
  useAddAliasMutation,
  useDeleteAliasMutation,
  useErrorText,
  type ChapterAlias,
  type TitleChapter,
} from "@/api";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";

type Props = {
  titleId: number;
  chapters: TitleChapter[];
  aliases: ChapterAlias[];
};

export function AliasesPanel({ titleId, chapters, aliases }: Props) {
  const t = useT();
  const add = useAddAliasMutation(titleId);
  const remove = useDeleteAliasMutation(titleId);
  const [aliasKey, setAliasKey] = useState("");
  const [canonicalKey, setCanonicalKey] = useState("");
  const [confirmKey, setConfirmKey] = useState<string | null>(null);

  const addError = useErrorText(add.error);
  const removeError = useErrorText(remove.error);
  const addSuccess = add.isSuccess ? t("admin.aliasAdded") : null;

  const keys = useMemo(() => {
    const values = new Set<string>();
    for (const chapter of chapters) {
      values.add(chapter.chapter_key);
      for (const variant of chapter.variants) values.add(variant.chapter_key);
    }
    return [...values].sort();
  }, [chapters]);

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const alias = aliasKey.trim();
    const canonical = canonicalKey.trim();
    if (alias === "" || canonical === "") return;
    add.mutate(
      { aliasKey: alias, canonicalKey: canonical },
      {
        onSuccess: () => {
          setAliasKey("");
          setCanonicalKey("");
        },
      },
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("admin.aliases")}</CardTitle>
        <CardDescription>{t("admin.aliasesHint")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <datalist id="chapter-keys">
          {keys.map((key) => (
            <option key={key} value={key} />
          ))}
        </datalist>

        <form className="flex flex-col gap-2 sm:flex-row" onSubmit={handleSubmit}>
          <Input
            value={aliasKey}
            list="chapter-keys"
            placeholder={t("admin.aliasKey")}
            aria-label={t("admin.aliasKey")}
            onChange={(event) => setAliasKey(event.target.value)}
          />
          <Input
            value={canonicalKey}
            list="chapter-keys"
            placeholder={t("admin.canonicalKey")}
            aria-label={t("admin.canonicalKey")}
            onChange={(event) => setCanonicalKey(event.target.value)}
          />
          <Button type="submit" variant="outline" disabled={add.isPending}>
            {t("admin.addAlias")}
          </Button>
        </form>

        {addError && <p role="alert" className="text-sm text-destructive">{addError}</p>}
        {addSuccess && <p className="text-sm text-muted-foreground">{addSuccess}</p>}

        {aliases.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("admin.noAliases")}</p>
        ) : (
          <ul className="divide-y divide-border">
            {aliases.map((alias) => (
              <li
                key={alias.alias_key}
                className="flex items-center justify-between gap-3 py-2"
              >
                <span className="min-w-0 truncate text-sm">
                  <span className="font-medium">{alias.alias_key}</span>
                  <span className="text-muted-foreground"> → </span>
                  <span>{alias.canonical_key}</span>
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant={confirmKey === alias.alias_key ? "destructive" : "outline"}
                  disabled={remove.isPending}
                  onClick={() => {
                    if (confirmKey === alias.alias_key) {
                      remove.mutate(alias.alias_key);
                      setConfirmKey(null);
                    } else {
                      setConfirmKey(alias.alias_key);
                    }
                  }}
                >
                  {confirmKey === alias.alias_key
                    ? t("admin.confirmDelete")
                    : t("admin.deleteAlias")}
                </Button>
              </li>
            ))}
          </ul>
        )}

        {removeError && <p role="alert" className="text-sm text-destructive">{removeError}</p>}
      </CardContent>
    </Card>
  );
}
