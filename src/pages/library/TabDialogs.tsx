import { type FormEvent, useState } from "react";

import {
  useCreateLibraryTabMutation,
  useDeleteLibraryTabMutation,
  useErrorText,
  useLibraryTabsQuery,
  useRenameLibraryTabMutation,
  type LibraryTab,
} from "@/api";
import { useT } from "@/lib/i18n";
import { Button } from "@/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui/shadcn/dialog";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/ui/shadcn/empty";
import { Input } from "@/ui/shadcn/input";

const MAX_NAME_LENGTH = 40;

type DialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function CreateTabDialog({ open, onOpenChange }: DialogProps) {
  const t = useT();
  const [name, setName] = useState("");
  const create = useCreateLibraryTabMutation();
  const errorText = useErrorText(create.error);
  const trimmed = name.trim();

  function handleOpenChange(next: boolean): void {
    if (!next) {
      setName("");
      create.reset();
    }
    onOpenChange(next);
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (trimmed === "" || create.isPending) return;
    create.mutate(trimmed, { onSuccess: () => handleOpenChange(false) });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("library.tabs.createTitle")}</DialogTitle>
          <DialogDescription>{t("library.tabs.createDescription")}</DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-3" onSubmit={handleSubmit}>
          <Input
            autoFocus
            value={name}
            maxLength={MAX_NAME_LENGTH}
            placeholder={t("library.tabs.name")}
            aria-label={t("library.tabs.name")}
            onChange={(event) => setName(event.target.value)}
          />
          {errorText && (
            <p role="alert" className="text-sm text-destructive">
              {errorText}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
            >
              {t("title.cancel")}
            </Button>
            <Button type="submit" disabled={trimmed === "" || create.isPending}>
              {t("library.tabs.create")}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function TabRow({ tab }: { tab: LibraryTab }) {
  const t = useT();
  const [name, setName] = useState(tab.name);
  const [confirming, setConfirming] = useState(false);
  const rename = useRenameLibraryTabMutation();
  const remove = useDeleteLibraryTabMutation();
  const errorText = useErrorText(rename.error ?? remove.error);
  const pending = rename.isPending || remove.isPending;
  const trimmed = name.trim();
  const dirty = trimmed !== tab.name;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <Input
          value={name}
          maxLength={MAX_NAME_LENGTH}
          aria-label={t("library.tabs.name")}
          onChange={(event) => setName(event.target.value)}
        />
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={!dirty || trimmed === "" || pending}
          onClick={() => rename.mutate({ tabId: tab.id, name: trimmed })}
        >
          {t("library.tabs.save")}
        </Button>
        {confirming ? (
          <Button
            type="button"
            size="sm"
            variant="destructive"
            disabled={pending}
            onClick={() => remove.mutate(tab.id)}
          >
            {t("library.tabs.confirmDelete")}
          </Button>
        ) : (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={pending}
            onClick={() => setConfirming(true)}
          >
            {t("library.tabs.delete")}
          </Button>
        )}
      </div>
      <span className="text-xs text-muted-foreground">
        {t("library.tabs.count", { count: tab.count })}
      </span>
      {errorText && (
        <p role="alert" className="text-xs text-destructive">
          {errorText}
        </p>
      )}
    </div>
  );
}

export function ManageTabsDialog({ open, onOpenChange }: DialogProps) {
  const t = useT();
  const tabsQuery = useLibraryTabsQuery();
  const tabs = tabsQuery.data?.tabs ?? [];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("library.tabs.manageTitle")}</DialogTitle>
          <DialogDescription>{t("library.tabs.manageDescription")}</DialogDescription>
        </DialogHeader>
        {tabs.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t("library.tabs.empty")}</EmptyTitle>
              <EmptyDescription>{t("library.tabs.emptyDescription")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto">
            {tabs.map((tab) => (
              <TabRow key={tab.id} tab={tab} />
            ))}
          </div>
        )}
        <div className="flex justify-end">
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            {t("title.cancel")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
