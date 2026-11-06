export const ru = {
  common: {
    nav: {
      home: "Главная",
      network: "Сеть",
      apps: "Приложения",
      utils: "Утилиты",
      system: "Система",
      groups: {
        overview: "Обзор",
        workshop: "Мастерская",
      },
      changelog: "Что нового",
      settings: "Настройки",
      unknown: "Неизвестно",
      toggleSidebar: "Переключить сайдбар",
      sidebar: "Боковое меню",
      sidebarDescription: "Навигация по приложению.",
    },
    theme: {
      toggle: "Переключить тему",
    },
    locale: {
      toggle: "Переключить язык",
      name: { en: "English", ru: "Русский" },
    },
    shell: {
      navbar: {
        language: "Язык",
      },
    },
    network: {
      autoRefresh: "Авто · {{seconds}} с",
      proxyLine: "прокси {{server}}",
      status: {
        online: "Онлайн",
        offline: "Нет доступа",
        checking: "Проверка…",
      },
      vpn: {
        tun: "TUN",
        proxy: "Прокси",
        off: "VPN выкл",
      },
      ip: {
        title: "IP-адреса",
        external: "Внешний IP",
        local: "Локальный IP",
        copy: "Скопировать {{value}}",
        copied: "Скопировано",
        copyFailed: "Не удалось скопировать",
      },
      public: {
        error: "Не удалось получить",
        notDetected: "IP не определён",
      },
      checks: {
        title: "Проверка сервисов",
        internet: "Интернет",
        chatgpt: "ChatGPT",
        localDns: "DNS",
        updated: "Обновлено в {{time}}",
        latency: "{{ms}} мс",
        ok: "OK",
        refused: "{{code}} · отказ",
        timeout: "таймаут",
        noAnswer: "нет ответа",
        proxyUnavailable: "прокси недоступен",
        dnsFailed: "DNS не отвечает",
        tlsFailed: "ошибка TLS",
        headers: {
          service: "Сервис",
          direct: "Напрямую",
          proxy: "Прокси",
          result: "Результат",
        },
        tooltips: {
          internet:
            "TCP-проверка: есть ли выход в сеть. Не проверяет DNS, TLS и конкретный сайт.",
          chatgpt:
            "HTTPS-запрос к chatgpt.com. 403 — сервис отказал, хотя интернет может работать.",
          dns: "Резолв системным DNS. При системном прокси имена может резолвить сам прокси.",
        },
      },
      mock: {
        label: "Демо",
        note: "Демо-данные: сетевые проверки пока не выполняются.",
        states: {
          proxy: "прокси",
          tun: "TUN",
          vpnOff: "без VPN",
          offline: "офлайн",
        },
      },
    },
    changelog: {
      description: "Список изменений по версиям.",
      empty: "Релизов пока нет",
      emptyDescription: "Список изменений появится здесь после первого релиза.",
    },
  },
};
