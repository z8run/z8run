import { assertAuthResponse } from "@/lib/validation";
import ky from "ky";

const authApi = ky.create({
  prefixUrl: "/auth",
  timeout: 10000,
  // Send/receive the HttpOnly session cookie (SEC-009).
  credentials: "include",
});

export interface AuthResponse {
  token: string;
  user: {
    id: string;
    email: string;
    username: string;
    roles: string[];
  };
}

export interface UserInfo {
  id: string;
  email: string;
  username: string;
  roles: string[];
}

export const authService = {
  register: (email: string, username: string, password: string) =>
    authApi
      .post("register", { json: { email, username, password } })
      .json<unknown>()
      .then(assertAuthResponse),

  login: (email: string, password: string) =>
    authApi
      .post("login", { json: { email, password } })
      .json<unknown>()
      .then(assertAuthResponse),

  // Auth is carried by the session cookie; no token argument needed.
  me: () => authApi.get("me").json<UserInfo>(),

  logout: () => authApi.post("logout").json<{ status: string }>(),
};
