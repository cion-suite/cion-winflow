import { useLocale, useT } from "@/lib/i18n";
import { CopyButton } from "./CopyButton";
import { countryName } from "./format";
import type { IpEntry } from "@/api";
import { StatusDot } from "./StatusDot";

type IpTileProps = {
  label: string;
  entry: IpEntry;
};

export function IpTile({ label, entry }: IpTileProps) {
  const t = useT();
  const locale = useLocale();
  const value = entry.value ?? "—";
  const detail = entry.countryCode
    ? `${entry.countryCode} · ${countryName(entry.countryCode, locale)}`
    : (entry.alias ?? "—");

  return (
    <div className="min-w-0 rounded-lg bg-muted/50 px-3 py-3 text-center">
      <div className="mb-1 flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
        <StatusDot
          tone={entry.tone}
          label={`${label}: ${t(`network.tone.${entry.tone}`)}`}
          ring
        />
        <span className="min-w-0 truncate">{label}</span>
      </div>
      <div className="flex min-h-6 items-center justify-center gap-1">
        <CopyButton key={value} value={entry.value} label={t("network.ip.copy", { value })} />
        <span
          title={entry.value ?? undefined}
          className="min-w-0 truncate text-sm font-semibold tabular-nums tracking-tight"
        >
          {value}
        </span>
      </div>
      <p className="mt-1 text-xs text-muted-foreground [overflow-wrap:anywhere]">{detail}</p>
    </div>
  );
}
