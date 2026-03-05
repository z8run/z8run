import ky from "ky";

export const api = ky.create({
  prefixUrl: "/api/v1",
  timeout: 10000,
  retry: 2,
});
