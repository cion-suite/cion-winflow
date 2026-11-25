import { authApi } from "./auth";
import { catalogApi } from "./catalog";
import { libraryApi } from "./library";

// По мере появления областей API их функции собираются здесь.
export const api = { auth: authApi, catalog: catalogApi, library: libraryApi };

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
  CoverResponse,
  LibraryState,
  Title,
  TitleInput,
  TitlePayload,
  TitleResponse,
  TitleSummary,
} from "./catalog";
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
