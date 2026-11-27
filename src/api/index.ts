import { adminApi } from "./admin";
import { aliasRangesApi } from "./aliasRanges";
import { authApi } from "./auth";
import { catalogApi } from "./catalog";
import { chaptersApi } from "./chapters";
import { discoverApi } from "./discover";
import { libraryApi } from "./library";
import { profileApi } from "./profile";
import { progressApi } from "./progress";
import { sourceCredentialsApi } from "./sourceCredentials";

// По мере появления областей API их функции собираются здесь.
export const api = {
  admin: adminApi,
  aliasRanges: aliasRangesApi,
  auth: authApi,
  catalog: catalogApi,
  chapters: chaptersApi,
  discover: discoverApi,
  library: libraryApi,
  profile: profileApi,
  progress: progressApi,
  sourceCredentials: sourceCredentialsApi,
};

// Компонентам и экранам api доступен только через эту точку входа
export {
  AuthProvider,
  useAuth,
  useChangePasswordMutation,
  useLoginMutation,
  useLogoutMutation,
  useRegisterMutation,
  useUpdateUsernameMutation,
} from "./auth";
export type { ChangePasswordInput, Role, User } from "./auth";
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
  BOOKMARK_IMPORT_QUERY_KEY,
  invalidateAfterBookmarkImport,
  profileApi,
  useBookmarkImportMutation,
  useBookmarkImportQuery,
  useDeleteAvatarMutation,
  useExportBookmarksMutation,
  useProfileQuery,
  useUploadAvatarMutation,
  userAvatarUrl,
} from "./profile";
export {
  SOURCE_CREDENTIALS_QUERY_KEY,
  sourceCredentialsApi,
  useDeleteSourceCredentialMutation,
  useLoginSourceCredentialMutation,
  useSourceCredentialsQuery,
  useVerifySourceCredentialMutation,
} from "./sourceCredentials";
export type {
  SourceCredential,
  SourceCredentialsResponse,
  SourceLoginInput,
} from "./sourceCredentials";
export type {
  AvatarResponse,
  BookmarkImportError,
  BookmarkImportJob,
  BookmarkImportResponse,
  BookmarkImportState,
  ProfileLibrary,
  ProfileQuery,
  ProfileResponse,
  ProfileStats,
  ProfileUser,
} from "./profile";
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
