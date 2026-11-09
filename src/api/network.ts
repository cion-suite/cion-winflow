import { useQuery } from "@tanstack/react-query";

import { call } from "./client";

export const NETWORK_SNAPSHOT_COMMAND = "network_snapshot";
export const NETWORK_REFRESH_MS = 30_000;

export type Tone = "ok" | "warning" | "error" | "muted";

export type IpEntry = {
  value: string | null;
  tone: Tone;
  countryCode: string | null;
  alias: string | null;
};

export type ProxyState = {
  enabled: boolean;
  host: string | null;
  port: number | null;
  portListening: boolean;
  connectStatus: number | null;
  connectLatencyMs: number | null;
  tone: Tone;
};

export type TunnelState = {
  adapter: string | null;
  up: boolean;
  sourceIp: string | null;
  routeViaTun: boolean;
  tone: Tone;
};

export type AccessState = {
  adapter: string | null;
  up: boolean;
  gateway: string | null;
  gatewayReachable: boolean;
  exitTarget: string;
  exitOk: boolean;
  exitLatencyMs: number | null;
  exitVia: string | null;
  dnsHost: string;
  dnsOk: boolean;
  dnsLatencyMs: number | null;
};

export type NetworkSnapshot = {
  browserIp: IpEntry;
  systemIp: IpEntry;
  localIp: IpEntry;
  proxy: ProxyState;
  tunnel: TunnelState;
  access: AccessState;
};

export function useNetworkSnapshot() {
  return useQuery<NetworkSnapshot, Error>({
    queryKey: [NETWORK_SNAPSHOT_COMMAND],
    queryFn: () => call<NetworkSnapshot>(NETWORK_SNAPSHOT_COMMAND),
    refetchInterval: NETWORK_REFRESH_MS,
    staleTime: NETWORK_REFRESH_MS,
  });
}
