import { api, NETWORK_REFRESH_MS } from "@/api";
import { useT } from "@/lib/i18n";
import { AccessCard } from "./AccessCard";
import { NetworkPanel } from "./NetworkPanel";
import { ProxyCard } from "./ProxyCard";
import { TunnelCard } from "./TunnelCard";

export function NetworkPage() {
  const t = useT();
  // Хук вызывается безусловно: api.* не распознаётся правилом хуков.
  const { data: snapshot, error } = api.network.useNetworkSnapshot();

  if (!snapshot) {
    return (
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-3">
        {error ? (
          <p
            role="alert"
            className="rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive"
          >
            {error.message}
          </p>
        ) : (
          <p role="status" className="text-sm text-muted-foreground">
            {t("network.loading")}
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-3">
      {error && (
        <p
          role="alert"
          className="rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive"
        >
          {error.message}
        </p>
      )}

      <NetworkPanel snapshot={snapshot} autoRefreshMs={NETWORK_REFRESH_MS} />
      <TunnelCard snapshot={snapshot} />
      <ProxyCard snapshot={snapshot} />
      <AccessCard snapshot={snapshot} />
    </div>
  );
}
