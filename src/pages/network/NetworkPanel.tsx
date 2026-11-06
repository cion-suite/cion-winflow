import { useLocale, useT } from "@/lib/i18n";
import { Badge } from "@/ui/shadcn/badge";
import { ChecksTable } from "./ChecksTable";
import { formatTime } from "./format";
import { IpSection } from "./IpSection";
import type { NetworkSnapshot } from "./mock";
import { getNetworkStatus } from "./status";

const VPN_KEYS = {
  tun: "network.vpn.tun",
  proxy: "network.vpn.proxy",
  off: "network.vpn.off",
} as const;

type NetworkPanelProps = {
  snapshot: NetworkSnapshot;
  autoRefreshMs: number;
};

export function NetworkPanel({ snapshot, autoRefreshMs }: NetworkPanelProps) {
  const t = useT();
  const locale = useLocale();
  const status = getNetworkStatus(snapshot);
  const proxyServer = snapshot.systemProxy.enabled ? snapshot.systemProxy.server : null;

  return (
    <section aria-labelledby="network-title" className="rounded-xl border bg-card text-sm">
      <header className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5 px-4 pb-3 pt-3">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h1 id="network-title" className="text-base font-semibold tracking-tight">
            {t("nav.network")}
          </h1>
          <Badge
            variant="secondary"
            title={proxyServer ? t("network.proxyLine", { server: proxyServer }) : undefined}
          >
            {t(VPN_KEYS[snapshot.vpnMode])}
          </Badge>
        </div>
        <span className="text-xs text-muted-foreground">
          {t("network.autoRefresh", { seconds: autoRefreshMs / 1000 })}
        </span>
      </header>

      <IpSection snapshot={snapshot} status={status} />

      <section aria-labelledby="network-checks-title" className="px-4">
        <header className="mb-1.5 flex items-center justify-between gap-2">
          <h2 id="network-checks-title" className="text-sm font-semibold">
            {t("network.checks.title")}
          </h2>
          <span className="text-xs text-muted-foreground">{t("network.mock.label")}</span>
        </header>
        <ChecksTable snapshot={snapshot} />
      </section>

      <footer className="mx-4 mt-1.5 flex items-center justify-end border-t border-border/70 pb-4 pt-2">
        <span className="text-xs text-muted-foreground">
          {t("network.checks.updated", { time: formatTime(snapshot.checkedAt, locale) })}
        </span>
      </footer>
    </section>
  );
}
