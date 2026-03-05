import { create } from "zustand";
import { authService, AuthResponse, UserInfo } from "@/api/auth";

interface AuthState {
  token: string | null;
  user: UserInfo | null;
  loading: boolean;
  error: string | null;

  login: (email: string, password: string) => Promise<void>;
  register: (email: string, username: string, password: string) => Promise<void>;
  logout: () => void;
  checkAuth: () => Promise<void>;
  clearError: () => void;
}

export const useAuthStore = create<AuthState>((set, get) => ({
  token: localStorage.getItem("z8_token"),
  user: null,
  loading: false,
  error: null,

  login: async (email, password) => {
    set({ loading: true, error: null });
    try {
      const res = await authService.login(email, password);
      localStorage.setItem("z8_token", res.token);
      set({ token: res.token, user: res.user, loading: false });
    } catch (err: any) {
      const msg = err?.response
        ? await err.response
            .json()
            .then((b: any) => b.error?.message || "Login failed")
            .catch(() => "Login failed")
        : "Network error";
      set({ error: msg, loading: false });
    }
  },

  register: async (email, username, password) => {
    set({ loading: true, error: null });
    try {
      const res = await authService.register(email, username, password);
      localStorage.setItem("z8_token", res.token);
      set({ token: res.token, user: res.user, loading: false });
    } catch (err: any) {
      const msg = err?.response
        ? await err.response
            .json()
            .then((b: any) => b.error?.message || "Registration failed")
            .catch(() => "Registration failed")
        : "Network error";
      set({ error: msg, loading: false });
    }
  },

  logout: () => {
    localStorage.removeItem("z8_token");
    set({ token: null, user: null });
  },

  checkAuth: async () => {
    const token = get().token;
    if (!token) return;
    try {
      const user = await authService.me(token);
      set({ user });
    } catch {
      // Token expired or invalid
      localStorage.removeItem("z8_token");
      set({ token: null, user: null });
    }
  },

  clearError: () => set({ error: null }),
}));
