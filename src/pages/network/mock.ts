export type MockScenario =
  | "proxy"
  | "tun"
  | "proxy-dead"
  | "tun-no-exit"
  | "vpn-off"
  | "offline";

export const MOCK_SCENARIOS: readonly MockScenario[] = [
  "proxy",
  "tun",
  "proxy-dead",
  "tun-no-exit",
  "vpn-off",
  "offline",
];

export function isMockScenario(value: string | null): value is MockScenario {
  return value !== null && (MOCK_SCENARIOS as readonly string[]).includes(value);
}

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
  adapter: string;
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

const ip = (
  value: string,
  countryCode: string | null = null,
  alias: string | null = null,
): IpEntry => ({ value, tone: "ok", countryCode, alias });

const missing = (): IpEntry => ({
  value: null,
  tone: "error",
  countryCode: null,
  alias: null,
});

const proxyOk: ProxyState = {
  enabled: true,
  host: "127.0.0.1",
  port: 2080,
  portListening: true,
  connectStatus: 200,
  connectLatencyMs: 41,
  tone: "ok",
};

const proxyOff: ProxyState = {
  enabled: false,
  host: null,
  port: null,
  portListening: false,
  connectStatus: null,
  connectLatencyMs: null,
  tone: "muted",
};

const tunnelOff: TunnelState = {
  adapter: null,
  up: false,
  sourceIp: null,
  routeViaTun: false,
  tone: "muted",
};

const tunnelOk: TunnelState = {
  adapter: "neko-tun",
  up: true,
  sourceIp: "172.19.0.1",
  routeViaTun: true,
  tone: "ok",
};

function access(overrides: Partial<AccessState> = {}): AccessState {
  return {
    adapter: "Ethernet",
    up: true,
    gateway: "192.168.1.1",
    gatewayReachable: true,
    exitTarget: "TCP 1.1.1.1:443",
    exitOk: true,
    exitLatencyMs: 34,
    exitVia: "Ethernet",
    dnsHost: "example.com",
    dnsOk: true,
    dnsLatencyMs: 12,
    ...overrides,
  };
}

export function createMockSnapshot(scenario: MockScenario): NetworkSnapshot {
  switch (scenario) {
    case "proxy":
      return {
        browserIp: ip("185.42.12.8", "NL"),
        systemIp: ip("212.57.103.70", "RU"),
        localIp: ip("192.168.1.42", null, "Ethernet"),
        proxy: proxyOk,
        tunnel: tunnelOff,
        access: access(),
      };
    case "tun":
      return {
        browserIp: ip("185.42.12.8", "NL"),
        systemIp: ip("185.42.12.8", "NL"),
        localIp: ip("192.168.1.42", null, "Ethernet"),
        proxy: proxyOk,
        tunnel: tunnelOk,
        access: access({ exitVia: "neko-tun" }),
      };
    case "proxy-dead":
      return {
        browserIp: missing(),
        systemIp: ip("212.57.103.70", "RU"),
        localIp: ip("192.168.1.42", null, "Ethernet"),
        proxy: {
          enabled: true,
          host: "127.0.0.1",
          port: 2080,
          portListening: false,
          connectStatus: null,
          connectLatencyMs: null,
          tone: "error",
        },
        tunnel: tunnelOff,
        access: access(),
      };
    case "tun-no-exit":
      // Локальный прокси-путь жив, а маршрут через TUN — нет:
      // сломан именно TUN-перехват, а не upstream клиента.
      return {
        browserIp: ip("185.42.12.8", "NL"),
        systemIp: missing(),
        localIp: ip("192.168.1.42", null, "Ethernet"),
        proxy: proxyOk,
        tunnel: { ...tunnelOk, tone: "error" },
        access: access({
          exitOk: false,
          exitLatencyMs: null,
          exitVia: "neko-tun",
        }),
      };
    case "vpn-off":
      return {
        browserIp: ip("212.57.103.70", "RU"),
        systemIp: ip("212.57.103.70", "RU"),
        localIp: ip("192.168.1.42", null, "Ethernet"),
        proxy: proxyOff,
        tunnel: tunnelOff,
        access: access(),
      };
    case "offline":
      return {
        browserIp: missing(),
        systemIp: missing(),
        localIp: ip("192.168.1.42", null, "Ethernet"),
        proxy: proxyOff,
        tunnel: tunnelOff,
        access: access({
          gatewayReachable: false,
          exitOk: false,
          exitLatencyMs: null,
          dnsOk: false,
          dnsLatencyMs: null,
        }),
      };
  }
}
