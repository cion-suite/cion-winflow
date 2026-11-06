import { useT } from "@/lib/i18n";
import { CheckLabel, CheckValue } from "./CheckValue";
import type { CheckResult, NetworkSnapshot } from "./mock";

const HEADER_FIRST =
  "w-[36%] border-b border-border/70 pb-1.5 pl-0 pr-2 pt-2 text-left text-xs font-medium text-muted-foreground";
const HEADER_VALUE =
  "border-b border-border/70 pb-1.5 pl-2 pr-0 pt-2 text-right text-xs font-medium text-muted-foreground";
const ROW_HEADER = "py-2 pl-0 pr-2 text-left text-sm font-normal";
const VALUE_CELL = "py-2 pl-2 pr-0 text-right text-sm";
const LABEL =
  "rounded-sm text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-foreground";

type CheckRow = {
  key: "internet" | "chatgpt";
  label: string;
  hint: string;
  results: (CheckResult | null)[];
};

export function ChecksTable({ snapshot }: { snapshot: NetworkSnapshot }) {
  const t = useT();
  const paths = snapshot.paths;
  const dual = paths.length > 1;

  const rows: CheckRow[] = [
    {
      key: "internet",
      label: t("network.checks.internet"),
      hint: t("network.checks.tooltips.internet"),
      results: paths.map((path) => path.internet),
    },
    {
      key: "chatgpt",
      label: t("network.checks.chatgpt"),
      hint: t("network.checks.tooltips.chatgpt"),
      results: paths.map((path) => path.chatgpt),
    },
  ];

  return (
    <table aria-labelledby="network-checks-title" className="w-full table-fixed border-collapse">
      <thead>
        <tr>
          <th scope="col" className={HEADER_FIRST}>
            {t("network.checks.headers.service")}
          </th>
          {dual ? (
            paths.map((path) => (
              <th key={path.kind} scope="col" className={HEADER_VALUE}>
                {t(
                  path.kind === "proxy"
                    ? "network.checks.headers.proxy"
                    : "network.checks.headers.direct",
                )}
              </th>
            ))
          ) : (
            <th scope="col" className={HEADER_VALUE}>
              {t("network.checks.headers.result")}
            </th>
          )}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row.key} className="border-b border-border/70 last:border-b-0">
            <th scope="row" className={ROW_HEADER}>
              <CheckLabel label={row.label} hint={row.hint} className={LABEL} />
            </th>
            {row.results.length > 0 ? (
              row.results.map((result, index) => (
                <td key={paths[index].kind} className={VALUE_CELL}>
                  <CheckValue result={result} />
                </td>
              ))
            ) : (
              // Путей ещё нет: держим колонку результата, пока идёт проверка.
              <td className={VALUE_CELL}>
                <CheckValue result={null} />
              </td>
            )}
          </tr>
        ))}
        <tr className="last:border-b-0">
          <th scope="row" className={ROW_HEADER}>
            <CheckLabel
              label={t("network.checks.localDns")}
              hint={t("network.checks.tooltips.dns")}
              className={LABEL}
            />
          </th>
          <td colSpan={dual ? 2 : 1} className={VALUE_CELL}>
            <CheckValue result={snapshot.dns} />
          </td>
        </tr>
      </tbody>
    </table>
  );
}
