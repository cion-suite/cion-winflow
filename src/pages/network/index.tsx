import { useEffect, useState } from "react";

import { useT } from "@/lib/i18n";
import { AccessCard } from "./AccessCard";
import { MockScenarioPicker } from "./MockScenarioPicker";
import { NetworkPanel } from "./NetworkPanel";
import { ProxyCard } from "./ProxyCard";
import { TunnelCard } from "./TunnelCard";
import {
  createMockSnapshot,
  isMockScenario,
  type MockScenario,
} from "./mock";

const AUTO_REFRESH_MS = 30_000;

/** Для макета: состояние открывается ссылкой #/network?mock=tun|proxy-dead|offline. */
function initialScenario(): MockScenario {
  if (!import.meta.env.DEV) return "proxy";

  // HashRouter держит и маршрут, и его query в hash: #/network?mock=offline.
  const hashQuery = window.location.hash.split("?")[1] ?? "";
  const value = new URLSearchParams(hashQuery || window.location.search).get("mock");
  return isMockScenario(value) ? value : "proxy";
}

export function NetworkPage() {
  const t = useT();
  const [scenario, setScenario] = useState<MockScenario>(initialScenario);
  const [snapshot, setSnapshot] = useState(() => createMockSnapshot(scenario));

  useEffect(() => {
    const timer = window.setInterval(() => {
      setSnapshot(createMockSnapshot(scenario));
    }, AUTO_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [scenario]);

  function handleScenarioChange(next: MockScenario) {
    setScenario(next);
    setSnapshot(createMockSnapshot(next));
  }

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-3">
      {import.meta.env.DEV && (
        <MockScenarioPicker value={scenario} onChange={handleScenarioChange} />
      )}

      <NetworkPanel snapshot={snapshot} autoRefreshMs={AUTO_REFRESH_MS} />
      <TunnelCard snapshot={snapshot} />
      <ProxyCard snapshot={snapshot} />
      <AccessCard snapshot={snapshot} />

      <p className="text-xs leading-relaxed text-muted-foreground">{t("network.mock.note")}</p>
    </div>
  );
}
