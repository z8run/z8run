import ky from "ky";

// Auth is carried by the HttpOnly session cookie (SEC-009): `credentials:
// "include"` sends it with every request, so no Authorization header or
// localStorage token is needed. Both /api and /auth are same-origin (the dev
// server proxies them), so the SameSite=Lax cookie is sent.
export const api = ky.create({
  prefixUrl: "/api/v1",
  timeout: 10000,
  retry: 2,
  credentials: "include",
  hooks: {
    afterResponse: [
      (_request, _options, response) => {
        if (response.status === 401) {
          window.location.href = "/login";
        }
      },
    ],
  },
});
