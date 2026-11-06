import type { NetworkSnapshot } from "./mock";

export type NetworkStatus = "online" | "offline" | "checking";

export const NETWORK_STATUS_KEYS: Record<NetworkStatus, string> = {
  online: "network.status.online",
  offline: "network.status.offline",
  checking: "network.status.checking",
};

/** Состояние определяет активный путь: интернет либо есть, либо нет. */
export function getNetworkStatus(snapshot: NetworkSnapshot): NetworkStatus {
  const activePath = snapshot.paths.find((path) => path.active);
  if (!activePath) return "checking";
  return activePath.internet.ok ? "online" : "offline";
}
