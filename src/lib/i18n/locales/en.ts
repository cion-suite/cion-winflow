export const en = {
  common: {
    nav: {
      home: "Home",
      network: "Network",
      apps: "Apps",
      utils: "Utilities",
      system: "System",
      groups: {
        overview: "Overview",
        workshop: "Workshop",
      },
      changelog: "What's new",
      settings: "Settings",
      unknown: "Unknown",
      toggleSidebar: "Toggle sidebar",
      sidebar: "Sidebar",
      sidebarDescription: "Application navigation.",
    },
    theme: {
      toggle: "Toggle theme",
    },
    locale: {
      toggle: "Toggle language",
      name: { en: "English", ru: "Русский" },
    },
    shell: {
      navbar: {
        language: "Language",
      },
    },
    network: {
      autoRefresh: "Auto · {{seconds}} s",
      proxyLine: "proxy {{server}}",
      status: {
        online: "Online",
        offline: "No access",
        checking: "Checking…",
      },
      vpn: {
        tun: "TUN",
        proxy: "Proxy",
        off: "VPN off",
      },
      ip: {
        title: "IP addresses",
        external: "External IP",
        local: "Local IP",
        copy: "Copy {{value}}",
        copied: "Copied",
        copyFailed: "Could not copy",
      },
      public: {
        error: "Could not fetch",
        notDetected: "IP not detected",
      },
      checks: {
        title: "Service checks",
        internet: "Internet",
        chatgpt: "ChatGPT",
        localDns: "DNS",
        updated: "Updated at {{time}}",
        latency: "{{ms}} ms",
        ok: "OK",
        refused: "{{code}} · refused",
        timeout: "timeout",
        noAnswer: "no answer",
        proxyUnavailable: "proxy unavailable",
        dnsFailed: "DNS not responding",
        tlsFailed: "TLS error",
        headers: {
          service: "Service",
          direct: "Direct",
          proxy: "Proxy",
          result: "Result",
        },
        tooltips: {
          internet:
            "TCP check: is there an internet path. Does not test DNS, TLS, or a specific site.",
          chatgpt:
            "HTTPS request to chatgpt.com. A 403 means the service refused, while the internet may still work.",
          dns: "Resolved by the system DNS. With a system proxy, the proxy may resolve names itself.",
        },
      },
      mock: {
        label: "Demo",
        note: "Demo data: network checks are not performed yet.",
        states: {
          proxy: "proxy",
          tun: "TUN",
          vpnOff: "VPN off",
          offline: "offline",
        },
      },
    },
    changelog: {
      description: "Release notes for every version.",
      empty: "No releases yet",
      emptyDescription: "Release notes will appear here after the first release.",
    },
  },
};
