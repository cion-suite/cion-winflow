import { authApi } from "./auth";

// По мере появления областей API их функции собираются здесь.
export const api = { auth: authApi };

// Компонентам и экранам api доступен только через эту точку входа
export {
  AuthProvider,
  useAuth,
  useLoginMutation,
  useLogoutMutation,
  useRegisterMutation,
} from "./auth";
