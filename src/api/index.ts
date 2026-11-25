import { authApi } from "./auth";
import { catalogApi } from "./catalog";
import { chaptersApi } from "./chapters";
import { libraryApi } from "./library";
import { progressApi } from "./progress";

// По мере появления областей API их функции собираются здесь.
export const api = {
  auth: authApi,
  catalog: catalogApi,
  chapters: chaptersApi,
  library: libraryApi,
  progress: progressApi,
};

// Компонентам и экранам api доступен только через эту точку входа
export {
  AuthProvider,
  useAuth,
  useLoginMutation,
  useLogoutMutation,
  useRegisterMutation,
} from "./auth";
export {
  formValuesToTitleInput,
  titleCoverUrl,
  titleToFormValues,
  useCatalogQuery,
  useCreateTitleMutation,
  useTitleQuery,
  useUpdateTitleMutation,
  useUploadCoverMutation,
} from "./catalog";
export type {
  CatalogQuery,
  CatalogResponse,
  ChapterVariant,
  CoverResponse,
  LibraryState,
  Title,
  TitleChapter,
  TitleInput,
  TitlePayload,
  TitleResponse,
  TitleSource,
  TitleSummary,
} from "./catalog";
export {
  chapterContentQueryKey,
  pageImageUrl,
  useChapterContentQuery,
} from "./chapters";
export type {
  ChapterContent,
  ChapterContentResponse,
  ChapterSourceRef,
  ContentPage,
} from "./chapters";
export {
  progressApi,
  progressQueryKey,
  useProgressQuery,
  useSaveProgressMutation,
} from "./progress";
export type { Progress, ProgressInput, ProgressResponse } from "./progress";
export { useErrorText } from "./errors";
export {
  useLibraryQuery,
  useRemoveLibraryMutation,
  useUpsertLibraryMutation,
} from "./library";
export type {
  LibraryEntry,
  LibraryInput,
  LibraryItem,
  LibraryListResponse,
  LibraryQuery,
} from "./library";
