import { useEffect, useState } from "react";
import { CheckIcon, CopyIcon, CopyXIcon } from "lucide-react";

import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { Button } from "@/ui/shadcn/button";
import { copyText } from "./clipboard";

const COPY_FEEDBACK_MS = 1600;

type CopyFeedback = "idle" | "copied" | "failed";

const FEEDBACK_ICONS = {
  idle: CopyIcon,
  copied: CheckIcon,
  failed: CopyXIcon,
} as const;

type CopyButtonProps = {
  value: string | null;
  label: string;
};

/** Копирует значение: значок подтверждает результат, тот же текст слышит скринридер. */
export function CopyButton({ value, label }: CopyButtonProps) {
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
