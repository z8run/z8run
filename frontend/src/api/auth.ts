import ky from "ky";

const authApi = ky.create({
  prefixUrl: "/auth",
  timeout: 10000,
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
      .json<AuthResponse>(),

  login: (email: string, password: string) =>
    authApi.post("login", { json: { email, password } }).json<AuthResponse>(),

  me: (token: string) =>
    authApi
      .get("me", { headers: { Authorization: `Bearer ${token}` } })
      .json<UserInfo>(),
};
