import { api } from "./client";
import type {
  FlowListResponse,
  FlowDetail,
  CreateFlowRequest,
  CreateFlowResponse,
} from "@/types/flow";

export interface SaveFlowRequest {
  name?: string;
  description?: string;
  canvas_nodes: unknown[];
  canvas_edges: unknown[];
  viewport: { x: number; y: number; zoom: number };
}

export const flowsApi = {
  list: () => api.get("flows").json<FlowListResponse>(),

  get: (id: string) => api.get(`flows/${id}`).json<FlowDetail>(),

  create: (data: CreateFlowRequest) =>
    api.post("flows", { json: data }).json<CreateFlowResponse>(),

  update: (id: string, data: SaveFlowRequest) =>
    api.put(`flows/${id}`, { json: data }).json<{ id: string; updated_at: string }>(),

  delete: (id: string) => api.delete(`flows/${id}`).json<{ deleted: string }>(),

  start: (id: string) =>
    api.post(`flows/${id}/start`).json<{
      flow_id: string;
      trace_id: string;
      status: string;
      node_map: Record<string, string>;
      routes?: { method: string; path: string }[];
    }>(),

  stop: (id: string) =>
    api.post(`flows/${id}/stop`).json<{ flow_id: string; status: string }>(),

  export: (id: string) =>
    api.get(`flows/${id}/export`).json<{ z8run_version: string; export_format: number; flow: unknown }>(),

  import: (data: unknown) =>
    api.post("flows/import", { json: data }).json<CreateFlowResponse>(),
};
