import { cn } from "@/lib/utils";
import type { Tone } from "@/api";

const TONE_CLASSES: Record<Tone, string> = {
  ok: "bg-success",
  warning: "bg-warning",
  error: "bg-destructive",
  muted: "bg-muted-foreground",
};

const RING_CLASSES: Record<Tone, string> = {
  ok: "ring-success/15",
  warning: "ring-warning/15",
  error: "ring-destructive/15",
  muted: "ring-muted-foreground/15",
};

type StatusDotProps = {
  tone: Tone;
  label: string;
  /** Кольцо как у старого индикатора в блоке IP. */
  ring?: boolean;
  className?: string;
};

/** Точка состояния: цвет — единственный визуальный носитель, текст остаётся в aria-label. */
export function StatusDot({ tone, label, ring = false, className }: StatusDotProps) {
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={cn(
        "size-2 shrink-0 rounded-full",
        TONE_CLASSES[tone],
        ring && "ring-4",
        ring && RING_CLASSES[tone],
        className,
      )}
    />
  );
}
