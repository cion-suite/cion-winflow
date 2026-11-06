import type { ReactNode } from "react";

type CheckRow = {
  label: string;
  value: ReactNode;
};

type CheckCardProps = {
  title: string;
  rows: CheckRow[];
};

/** Карточка проверки: только факты, без текстов ошибок и кнопок. */
export function CheckCard({ title, rows }: CheckCardProps) {
  return (
    <section className="rounded-xl border bg-card px-4 py-3 text-sm">
      <h2 className="text-sm font-semibold tracking-tight">{title}</h2>
      <dl className="mt-1.5 grid grid-cols-[88px_minmax(0,1fr)] gap-x-3 gap-y-1">
        {rows.map((row) => (
          <div key={row.label} className="contents">
            <dt className="text-muted-foreground">{row.label}</dt>
            <dd className="min-w-0 [overflow-wrap:anywhere]">{row.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
