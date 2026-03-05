import ky from "ky";

export const api = ky.create({
  prefixUrl: "/api/v1",
  timeout: 10000,
  retry: 2,
  hooks: {
    beforeRequest: [
      (request) => {
        const token = localStorage.getItem("z8_token");
        if (token) {
          request.headers.set("Authorization", `Bearer ${token}`);
        }
      },
    ],
    afterResponse: [
      (_request, _options, response) => {
        if (response.status === 401) {
          localStorage.removeItem("z8_token");
          window.location.href = "/login";
        }
      },
    ],
  },
});
