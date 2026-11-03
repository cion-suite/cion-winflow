import { useT } from "@/lib/i18n";
import { PageHeader } from "@/ui/PageHeader";

export function NetworkPage() {
  const t = useT();

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <PageHeader title={t("nav.network")} />
    </div>
  );
}
