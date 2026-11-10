import { useT } from "@/lib/i18n";
import { CheckCard } from "./CheckCard";
import type { NetworkSnapshot } from "./mock";

export function TunnelCard({ snapshot }: { snapshot: NetworkSnapshot }) {
  const t = useT();
  const { tunnel, access } = snapshot;

  const adapter = tunnel.adapter
    ? [tunnel.adapter, tunnel.up ? t("network.tunnel.up") : t("network.tunnel.down"), tunnel.sourceIp]
        .filter((part): part is string => part !== null)
        .join(" · ")
    : "—";

  const route = tunnel.routeViaTun
    ? t("network.tunnel.viaTun")
    : t("network.tunnel.viaAdapter", { adapter: access.adapter });

  return (
    <CheckCard
      title={t("network.tunnel.title")}
      rows={[
        { label: t("network.tunnel.adapter"), value: adapter },
        { label: t("network.tunnel.route"), value: route },
      ]}
    />
  );
}
