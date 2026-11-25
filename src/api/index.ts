import { adminApi } from "./admin";
import { authApi } from "./auth";
import { catalogApi } from "./catalog";
import { chaptersApi } from "./chapters";
import { libraryApi } from "./library";
import { progressApi } from "./progress";

// По мере появления областей API их функции собираются здесь.
export const api = {
  admin: adminApi,
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
  ChapterAlias,
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
  useAddAliasMutation,
  useAttachSourceMutation,
  useDeleteAliasMutation,
  useDetachSourceMutation,
  useProbeSourceMutation,
  useRefreshTitleMutation,
  useSaveSourcePrioritiesMutation,
  useSetPrimarySourceMutation,
  useSourcePrioritiesQuery,
  useSourceSearchMutation,
  useSourcesQuery,
  useToggleSourceMutation,
} from "./admin";
export type {
  AliasListResponse,
  AttachSourceInput,
  AttachSourceResponse,
  ProbeResponse,
  RefreshEntry,
  RefreshResponse,
  SourceListResponse,
  SourcePrioritiesResponse,
  SourcePriority,
  SourceSearchInput,
  SourceSearchItem,
  SourceSearchResponse,
  SourceState,
} from "./admin";
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
