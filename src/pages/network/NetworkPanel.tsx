import { useT } from "@/lib/i18n";
import { Badge } from "@/ui/shadcn/badge";
import { IpTile } from "./IpTile";
import type { NetworkSnapshot } from "@/api";
import { StatusDot } from "./StatusDot";

type NetworkPanelProps = {
  snapshot: NetworkSnapshot;
  autoRefreshMs: number;
};

export function NetworkPanel({ snapshot, autoRefreshMs }: NetworkPanelProps) {
  const t = useT();
  const showTunnelBadge = snapshot.tunnel.tone !== "muted";
  const showProxyBadge = snapshot.proxy.tone !== "muted";

  return (
    <section aria-labelledby="network-title" className="rounded-xl border bg-card text-sm">
      <header className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5 px-4 pb-3 pt-3">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h1 id="network-title" className="text-base font-semibold tracking-tight">
            {t("nav.network")}
          </h1>
          {showTunnelBadge && (
            <Badge variant="secondary">
              <StatusDot
                tone={snapshot.tunnel.tone}
                label={t(`network.tone.${snapshot.tunnel.tone}`)}
              />
              {t("network.badges.tunnel")}
            </Badge>
          )}
          {showProxyBadge && (
            <Badge variant="secondary">
              <StatusDot
                tone={snapshot.proxy.tone}
                label={t(`network.tone.${snapshot.proxy.tone}`)}
              />
              {t("network.badges.proxy")}
            </Badge>
          )}
        </div>
        <span className="text-xs text-muted-foreground">
          {t("network.autoRefresh", { seconds: autoRefreshMs / 1000 })}
        </span>
      </header>

      <section
        aria-label={t("network.ip.title")}
        className="mx-4 mb-4 grid grid-cols-3 gap-3 max-[520px]:grid-cols-1"
      >
        <IpTile label={t("network.ip.browser")} entry={snapshot.browserIp} />
        <IpTile label={t("network.ip.system")} entry={snapshot.systemIp} />
        <IpTile label={t("network.ip.local")} entry={snapshot.localIp} />
      </section>
    </section>
  );
}
