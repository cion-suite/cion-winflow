import type { TFunction } from "i18next";
import { CircleAlertIcon, CircleCheckIcon, CircleXIcon } from "lucide-react";

import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/shadcn/tooltip";
import type { CheckResult } from "./mock";

type CheckTone = "ok" | "warning" | "error" | "muted";

const TONE_CLASSES: Record<CheckTone, string> = {
  ok: "text-success",
  warning: "text-warning",
  error: "text-destructive",
  muted: "text-muted-foreground",
};

const TONE_ICONS = {
  ok: CircleCheckIcon,
  warning: CircleAlertIcon,
  error: CircleXIcon,
  muted: null,
} as const;

/** Текст и цвет результата проверки: показываются и в строке, и в подсказке. */
export function describeCheck(
  result: CheckResult | null,
  t: TFunction<"common">,
): { text: string; tone: CheckTone } {
  if (!result) {
    return { text: "—", tone: "muted" };
  }

  if (result.ok) {
    const text =
      result.latencyMs === null
        ? t("network.checks.ok")
        : t("network.checks.latency", { ms: result.latencyMs });
    return { text, tone: "ok" };
  }

  if (result.errorKind === "http" && result.statusCode !== null) {
    return { text: t("network.checks.refused", { code: result.statusCode }), tone: "warning" };
  }

  if (result.errorKind === "timeout") {
    return { text: t("network.checks.timeout"), tone: "error" };
  }
  if (result.errorKind === "proxy") {
    return { text: t("network.checks.proxyUnavailable"), tone: "error" };
  }
  if (result.errorKind === "dns") {
    return { text: t("network.checks.dnsFailed"), tone: "error" };
  }
  if (result.errorKind === "tls") {
    return { text: t("network.checks.tlsFailed"), tone: "error" };
  }

  return { text: t("network.checks.noAnswer"), tone: "error" };
}

export function CheckValue({ result }: { result: CheckResult | null }) {
  const t = useT();
  const { text, tone } = describeCheck(result, t);
  const Icon = TONE_ICONS[tone];

  return (
    <span className={cn("inline-flex min-w-0 items-center gap-1.5", TONE_CLASSES[tone])}>
      {Icon && <Icon className="size-3.5 shrink-0" aria-hidden="true" />}
      <span className="min-w-0 break-words">{text}</span>
    </span>
  );
}

type CheckLabelProps = {
  label: string;
  hint: string;
  className?: string;
};

/** Название проверки: при наведении и фокусе объясняет, что именно проверяется. */
export function CheckLabel({ label, hint, className }: CheckLabelProps) {
  return (
    <Tooltip>
      <TooltipTrigger
        type="button"
        aria-label={`${label}. ${hint}`}
        className={cn(
          "w-fit cursor-help text-left text-muted-foreground underline decoration-dotted underline-offset-4 transition-colors hover:text-foreground",
          className,
        )}
      >
        {label}
      </TooltipTrigger>
      <TooltipContent>{hint}</TooltipContent>
    </Tooltip>
  );
}
