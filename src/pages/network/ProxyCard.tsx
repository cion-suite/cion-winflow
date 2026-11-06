import { useT } from "@/lib/i18n";
import { CheckCard } from "./CheckCard";
import type { NetworkSnapshot } from "./mock";

export function ProxyCard({ snapshot }: { snapshot: NetworkSnapshot }) {
  const t = useT();
  const { proxy } = snapshot;

  const server = proxy.host ?? "—";
  const port =
    proxy.port !== null
      ? `${proxy.port} · ${
          proxy.portListening
            ? t("network.proxy.listening")
            : t("network.proxy.closed")
        }`
      : "—";
  const connect =
    proxy.connectStatus === null
      ? "—"
      : [
          String(proxy.connectStatus),
          proxy.connectLatencyMs !== null
            ? `${proxy.connectLatencyMs} ${t("network.ms")}`
            : null,
        ]
          .filter((part): part is string => part !== null)
          .join(" · ");

  return (
    <CheckCard
      title={t("network.proxy.title")}
      rows={[
        { label: t("network.proxy.server"), value: server },
        { label: t("network.proxy.port"), value: port },
        { label: t("network.proxy.connect"), value: connect },
      ]}
    />
  );
}
