export type MockScenario = "proxy" | "tun" | "vpn-off" | "offline";

export type CheckErrorKind = "timeout" | "proxy" | "dns" | "tls" | "http" | null;

export type CheckResult = {
  ok: boolean;
  latencyMs: number | null;
  statusCode: number | null;
  errorKind: CheckErrorKind;
};

export type PathKind = "direct" | "proxy";

export type PathStatus = {
  kind: PathKind;
  active: boolean;
  internet: CheckResult;
  chatgpt: CheckResult | null;
};

export type NetworkSnapshot = {
  interface: {
    alias: string;
    kind: "tun" | "ethernet" | "wifi" | "other";
    localIp: string;
  };
  vpnMode: "tun" | "proxy" | "off";
  systemProxy: { enabled: boolean; server: string | null };
  dnsHost: string;
  dns: CheckResult;
  paths: PathStatus[];
  publicInfo: { ip: string; countryCode: string } | null;
  publicError: CheckErrorKind;
  checkedAt: Date;
  chatgptCheckedAt: Date | null;
  publicFetchedAt: Date | null;
};

const ok = (latencyMs: number, statusCode: number | null = null): CheckResult => ({
  ok: true,
  latencyMs,
  statusCode,
  errorKind: null,
});

const refused = (statusCode: number, latencyMs: number): CheckResult => ({
  ok: false,
  latencyMs,
  statusCode,
  errorKind: "http",
});

const failed = (errorKind: Exclude<CheckErrorKind, null>): CheckResult => ({
  ok: false,
  latencyMs: null,
  statusCode: null,
  errorKind,
});

function secondsAgo(seconds: number): Date {
  return new Date(Date.now() - seconds * 1000);
}

export function createMockSnapshot(scenario: MockScenario): NetworkSnapshot {
  const now = new Date();

  switch (scenario) {
    case "proxy":
      return {
        interface: { alias: "Ethernet", kind: "ethernet", localIp: "192.168.1.42" },
        vpnMode: "proxy",
        systemProxy: { enabled: true, server: "127.0.0.1:2080" },
        dnsHost: "example.com",
        dns: ok(12),
        paths: [
          {
            kind: "direct",
            active: false,
            internet: ok(34),
            chatgpt: refused(403, 120),
          },
          {
            kind: "proxy",
            active: true,
            internet: ok(2),
            chatgpt: ok(340, 200),
          },
        ],
        publicInfo: { ip: "185.42.12.8", countryCode: "NL" },
        publicError: null,
        checkedAt: now,
        chatgptCheckedAt: secondsAgo(20),
        publicFetchedAt: secondsAgo(240),
      };
    case "tun":
      return {
        interface: { alias: "nekoray-tun", kind: "tun", localIp: "198.18.0.1" },
        vpnMode: "tun",
        systemProxy: { enabled: false, server: null },
        dnsHost: "example.com",
        dns: ok(14),
        paths: [
          { kind: "direct", active: true, internet: ok(38), chatgpt: ok(410, 200) },
        ],
        publicInfo: { ip: "185.42.12.8", countryCode: "NL" },
        publicError: null,
        checkedAt: now,
        chatgptCheckedAt: secondsAgo(15),
        publicFetchedAt: secondsAgo(360),
      };
    case "vpn-off":
      return {
        interface: { alias: "Ethernet", kind: "ethernet", localIp: "192.168.1.42" },
        vpnMode: "off",
        systemProxy: { enabled: false, server: null },
        dnsHost: "example.com",
        dns: ok(11),
        paths: [
          { kind: "direct", active: true, internet: ok(31), chatgpt: refused(403, 95) },
        ],
        publicInfo: { ip: "212.57.103.70", countryCode: "RU" },
        publicError: null,
        checkedAt: now,
        chatgptCheckedAt: secondsAgo(25),
        publicFetchedAt: secondsAgo(480),
      };
    case "offline":
      return {
        interface: { alias: "nekoray-tun", kind: "tun", localIp: "198.18.0.1" },
        vpnMode: "tun",
        systemProxy: { enabled: false, server: null },
        dnsHost: "example.com",
        dns: ok(10),
        paths: [
          {
            kind: "direct",
            active: true,
            internet: failed("timeout"),
            chatgpt: failed("timeout"),
          },
        ],
        publicInfo: null,
        publicError: "timeout",
        checkedAt: now,
        chatgptCheckedAt: secondsAgo(30),
        publicFetchedAt: null,
      };
  }
}

/** Автообновление обновляет только быстрые проверки, не ChatGPT и не внешний IP. */
export function withFreshChecks(snapshot: NetworkSnapshot): NetworkSnapshot {
  return { ...snapshot, checkedAt: new Date() };
}
