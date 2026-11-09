import { useNetworkSnapshot } from "./network";

export type {
  AccessState,
  IpEntry,
  NetworkSnapshot,
  ProxyState,
  Tone,
  TunnelState,
} from "./network";
export { NETWORK_REFRESH_MS } from "./network";

export const api = {
  network: { useNetworkSnapshot },
};
