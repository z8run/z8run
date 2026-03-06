import { api } from "./client";

export interface VaultKey {
  key: string;
}

export const vaultApi = {
  list: () => api.get("vault").json<{ keys: string[] }>(),

  store: (key: string, value: string) =>
    api
      .post("vault", { json: { key, value } })
      .json<{ status: string; key: string }>(),

  get: (key: string) =>
    api.get(`vault/${key}`).json<{ key: string; value: string }>(),

  delete: (key: string) =>
    api.delete(`vault/${key}`).json<{ status: string; key: string }>(),
};
