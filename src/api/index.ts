import { adminApi } from "./admin";
import { aliasRangesApi } from "./aliasRanges";
import { authApi } from "./auth";
import { catalogApi } from "./catalog";
import { chaptersApi } from "./chapters";
import { discoverApi } from "./discover";
import { libraryApi } from "./library";
import { progressApi } from "./progress";

// По мере появления областей API их функции собираются здесь.
export const api = {
  admin: adminApi,
  aliasRanges: aliasRangesApi,
  auth: authApi,
  catalog: catalogApi,
  chapters: chaptersApi,
  discover: discoverApi,
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
  useCreateTitleMutation,
  useDeleteCoverMutation,
  useFetchSourceCoversMutation,
  useSyncTitleMutation,
  useTitleQuery,
  useUpdateTitleMutation,
  useUploadCoverMutation,
} from "./catalog";
export type {
  ChapterAlias,
  ChapterVariant,
  CoverResponse,
  FetchSourceCoversInput,
  LibraryState,
  SourceCoverError,
  SourceCoversResponse,
  Title,
  TitleChapter,
  TitleInput,
  TitlePayload,
  TitleResponse,
  TitleSource,
  TitleSummary,
  TitleSyncEntry,
  TitleSyncResponse,
  UploadCoverInput,
} from "./catalog";
export {
  aliasRangesApi,
  useAliasRangesQuery,
  useApplyAliasRangesMutation,
  useCreateAliasRangeMutation,
  useDeleteAliasRangeMutation,
  useUpdateAliasRangeMutation,
} from "./aliasRanges";
export type {
  AliasManualLink,
  AliasRange,
  AliasRangeAnchor,
  AliasRangeBatchInput,
  AliasRangeCoverage,
  AliasRangeInput,
  AliasRangeListResponse,
  AliasRangeMappingMode,
  AliasRangeStatus,
} from "./aliasRanges";
export {
  useAddAliasesMutation,
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
  AliasBatchInput,
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
  useSetChapterReadMutation,
} from "./chapters";
export type {
  ChapterContent,
  ChapterContentResponse,
  ChapterReadsInput,
  ChapterReadsResponse,
  ChapterSourceRef,
  ContentPage,
} from "./chapters";
export {
  discoverApi,
  sourceCoverUrl,
  useAddDiscoveredTitleMutation,
  useDiscoverSearchQuery,
} from "./discover";
export type {
  DiscoverAddInput,
  DiscoverAddResponse,
  DiscoverItem,
  DiscoverSearchResponse,
  DiscoverSourceError,
} from "./discover";
export {
  progressApi,
  progressQueryKey,
  useProgressQuery,
  useSaveProgressMutation,
} from "./progress";
export type { Progress, ProgressInput, ProgressResponse } from "./progress";
export { useErrorText } from "./errors";
export {
  useCreateLibraryTabMutation,
  useDeleteLibraryTabMutation,
  useLibraryQuery,
  useLibraryTabsQuery,
  useRemoveLibraryMutation,
  useRenameLibraryTabMutation,
  useUpsertLibraryMutation,
} from "./library";
export type {
  LibraryCounts,
  LibraryEntry,
  LibraryInput,
  LibraryItem,
  LibraryListResponse,
  LibraryOrder,
  LibraryQuery,
  LibrarySort,
  LibraryTab,
  LibraryTabsResponse,
  StatusCounts,
} from "./library";
