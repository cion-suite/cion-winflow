import type { TitleChapter, TitleSource } from "@/api";
import { AliasRangesPanel } from "./AliasRangesPanel";

type Props = {
  titleId: number;
  sources: TitleSource[];
  chapters: TitleChapter[];
  preferredSourceId: number | null;
  /** Язык просмотра: настройки показываются для выбранного перевода. */
  lang: string | null;
};

export function TitleSettingsTab(props: Props) {
  return <AliasRangesPanel {...props} />;
}
