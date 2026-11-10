import { useT } from "@/lib/i18n";
import { CheckCard } from "./CheckCard";
import type { NetworkSnapshot } from "./mock";

export function AccessCard({ snapshot }: { snapshot: NetworkSnapshot }) {
  const t = useT();
  const { access } = snapshot;

  const localIp = snapshot.localIp.value ?? "—";
  const adapter = `${access.adapter} · ${
    access.up ? t("network.access.up") : t("network.access.down")
  } · ${localIp}`;

  const gateway = access.gateway
    ? `${access.gateway} · ${
        access.gatewayReachable
          ? t("network.access.reachable")
          : t("network.access.unreachable")
      }`
    : "—";

  const exit = access.exitOk
    ? [
        access.exitTarget,
        access.exitLatencyMs !== null
          ? `${access.exitLatencyMs} ${t("network.ms")}`
          : "—",
        access.exitVia,
      ]
        .filter((part): part is string => part !== null)
        .join(" · ")
    : [t("network.access.timeout"), access.exitVia]
        .filter((part): part is string => part !== null)
        .join(" · ");

  const dns = access.dnsOk
    ? `${access.dnsHost} · ${
        access.dnsLatencyMs !== null ? `${access.dnsLatencyMs} ${t("network.ms")}` : "—"
      }`
    : `${access.dnsHost} · ${t("network.access.noAnswer")}`;

  return (
    <CheckCard
      title={t("network.access.title")}
      rows={[
        { label: t("network.access.adapter"), value: adapter },
        { label: t("network.access.gateway"), value: gateway },
        { label: t("network.access.exit"), value: exit },
        { label: t("network.access.dns"), value: dns },
      ]}
    />
  );
}
