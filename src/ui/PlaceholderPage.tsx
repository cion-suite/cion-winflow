import { useT } from "@/lib/i18n";
import { PageHeader } from "@/ui/PageHeader";

type PlaceholderPageProps = {
  titleKey: string;
};

/** Временный экран: маршрут уже работает, содержимое появится в следующих фазах. */
export function PlaceholderPage({ titleKey }: PlaceholderPageProps) {
  const t = useT();

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <PageHeader title={t(titleKey)} description={t("placeholder")} />
    </div>
  );
}
