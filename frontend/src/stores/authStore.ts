import { type UserInfo, authService } from "@/api/auth";
import { extractErrorMessage } from "@/lib/extractError";
import { create } from "zustand";

interface AuthState {
  user: UserInfo | null;
  loading: boolean;
  error: string | null;
  /** True once the initial session check (/auth/me) has completed. */
  initialized: boolean;

  login: (email: string, password: string) => Promise<void>;
  register: (
    email: string,
    username: string,
    password: string,
  ) => Promise<void>;
  logout: () => Promise<void>;
  checkAuth: () => Promise<void>;
  clearError: () => void;
}

// The session lives in an HttpOnly cookie set by the server (SEC-009); the
// token is never stored in JS. Auth state is derived from /auth/me instead.
export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  loading: false,
  error: null,
  initialized: false,

  login: async (email, password) => {
    set({ loading: true, error: null });
    try {
      const res = await authService.login(email, password);
      set({ user: res.user, loading: false, initialized: true });
    } catch (err: unknown) {
      const msg = await extractErrorMessage(err, "Login failed");
      set({ error: msg, loading: false });
    }
  },

  register: async (email, username, password) => {
    set({ loading: true, error: null });
    try {
      const res = await authService.register(email, username, password);
      set({ user: res.user, loading: false, initialized: true });
    } catch (err: unknown) {
      const msg = await extractErrorMessage(err, "Registration failed");
      set({ error: msg, loading: false });
    }
  },

  logout: async () => {
    try {
      await authService.logout();
    } catch {
      // Clear local state even if the network call fails.
    }
    set({ user: null });
  },

  checkAuth: async () => {
    try {
      // 200 with { user } (null if no session) — no 401, so no console error.
      const { user } = await authService.session();
      set({ user, initialized: true });
    } catch {
      // Network error only.
      set({ user: null, initialized: true });
    }
  },

  clearError: () => set({ error: null }),
}));
