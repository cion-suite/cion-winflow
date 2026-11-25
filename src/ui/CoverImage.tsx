import { useState } from "react";
import { ImageOffIcon } from "lucide-react";

import { cn } from "@/lib/utils";

type CoverImageProps = {
  src: string | null;
  alt: string;
  className?: string;
  loading?: "lazy" | "eager";
};

/** Обложка с заглушкой: отсутствующий файл не должен показывать битую картинку. */
export function CoverImage({ src, alt, className, loading = "lazy" }: CoverImageProps) {
  // Храним именно упавший URL: новый src автоматически считается рабочим
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const failed = src !== null && failedSrc === src;

  if (!src || failed) {
    return (
      <div
        role="img"
        aria-label={alt}
        className={cn(
          "flex items-center justify-center bg-muted text-muted-foreground",
          className,
        )}
      >
        <ImageOffIcon className="size-8" aria-hidden />
      </div>
    );
  }

  return (
    <img
      src={src}
      alt={alt}
      loading={loading}
      className={cn("object-cover", className)}
      onError={() => setFailedSrc(src)}
    />
  );
}
