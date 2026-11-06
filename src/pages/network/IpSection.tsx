import { useEffect, useState } from "react";
import { CheckIcon, CopyIcon, CopyXIcon } from "lucide-react";

import { useLocale, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/ui/shadcn/button";
import { copyText } from "./clipboard";
import { countryName } from "./format";
import type { NetworkSnapshot } from "./mock";
import { NETWORK_STATUS_KEYS, type NetworkStatus } from "./status";

const COPY_FEEDBACK_MS = 1600;

type CopyFeedback = "idle" | "copied" | "failed";

const FEEDBACK_ICONS = {
  idle: CopyIcon,
  copied: CheckIcon,
  failed: CopyXIcon,
} as const;

const STATUS_DOT_CLASSES: Record<NetworkStatus, string> = {
  online: "bg-success ring-success/15",
  offline: "bg-destructive ring-destructive/15",
  checking: "bg-muted-foreground ring-muted-foreground/15 motion-safe:animate-pulse",
};

type CopyButtonProps = {
  value: string | null;
  label: string;
};

/** Копирует значение: значок подтверждает результат, тот же текст слышит скринридер. */
function CopyButton({ value, label }: CopyButtonProps) {
  const t = useT();
  const [feedback, setFeedback] = useState<CopyFeedback>("idle");
  const [attempt, setAttempt] = useState(0);
  const Icon = FEEDBACK_ICONS[feedback];

  // attempt перезапускает таймер, даже если результат копирования не изменился
  useEffect(() => {
    if (feedback === "idle") return;
    const timer = window.setTimeout(() => setFeedback("idle"), COPY_FEEDBACK_MS);
    return () => window.clearTimeout(timer);
  }, [feedback, attempt]);

  async function handleCopy() {
    if (value === null) return;

    try {
      await copyText(value);
      setFeedback("copied");
    } catch {
      setFeedback("failed");
    }
    setAttempt((current) => current + 1);
  }

  return (
    <>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        disabled={value === null}
        aria-label={label}
        title={label}
        onClick={() => void handleCopy()}
        className={cn(
          "text-muted-foreground focus-visible:ring-foreground/60",
          feedback === "copied" && "text-success",
          feedback === "failed" && "text-destructive",
        )}
      >
        <Icon aria-hidden="true" />
      </Button>
      <span role="status" className="sr-only">
        {feedback === "copied" ? t("network.ip.copied") : null}
        {feedback === "failed" ? t("network.ip.copyFailed") : null}
      </span>
    </>
  );
}

function StatusDot({ status }: { status: NetworkStatus }) {
  const t = useT();
  const label = t(NETWORK_STATUS_KEYS[status]);

  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={cn("size-2 shrink-0 rounded-full ring-4", STATUS_DOT_CLASSES[status])}
    />
  );
}

type IpItemProps = {
  label: string;
  ip: string | null;
  detail: string;
  status?: NetworkStatus;
};

function IpItem({ label, ip, detail, status }: IpItemProps) {
  const t = useT();
  const value = ip ?? "—";

  return (
    <div className="min-w-0 text-center">
      <div className="mb-1 text-xs text-muted-foreground">{label}</div>
      <div className="flex min-h-6 items-center justify-center gap-1">
        <CopyButton key={value} value={ip} label={t("network.ip.copy", { value })} />
        <span
          title={ip ?? undefined}
          className="min-w-0 truncate text-sm font-semibold tabular-nums tracking-tight"
        >
          {value}
        </span>
        {status && <StatusDot status={status} />}
      </div>
      <p className="mt-1 text-xs text-muted-foreground [overflow-wrap:anywhere]">{detail}</p>
    </div>
  );
}

type IpSectionProps = {
  snapshot: NetworkSnapshot;
  status: NetworkStatus;
};

export function IpSection({ snapshot, status }: IpSectionProps) {
  const t = useT();
  const locale = useLocale();
  const info = snapshot.publicInfo;

  const countryDetail = info
    ? `${info.countryCode} · ${countryName(info.countryCode, locale)}${
        snapshot.publicError ? ` · ${t("network.public.error")}` : ""
      }`
    : t(snapshot.publicError ? "network.public.error" : "network.public.notDetected");

  return (
    <section aria-label={t("network.ip.title")} className="mx-4 mb-4 rounded-lg bg-muted/50 px-3 py-3">
      <div className="mx-auto grid w-2/3 grid-cols-2 gap-x-4 gap-y-3 max-[520px]:w-full max-[380px]:grid-cols-1">
        <IpItem
          label={t("network.ip.external")}
          ip={info?.ip ?? null}
          detail={countryDetail}
          status={status}
        />
        <IpItem
          label={t("network.ip.local")}
          ip={snapshot.interface.localIp}
          detail={snapshot.interface.alias}
        />
      </div>
    </section>
  );
}
