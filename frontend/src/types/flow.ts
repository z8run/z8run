export type PortType =
  | "any"
  | "string"
  | "number"
  | "boolean"
  | "object"
  | "array"
  | "binary";

export type NodeCategory =
  | "input"
  | "process"
  | "output"
  | "logic"
  | "data"
  | "ai";

export type NodeStatus = "idle" | "running" | "success" | "error" | "disabled";

export type FlowStatus =
  | "idle"
  | "running"
  | "paused"
  | "completed"
  | "error"
  | "stopped";

export interface PortDefinition {
  id: string;
  name: string;
  type: PortType;
  required?: boolean;
}

export interface Z8NodeData {
  [key: string]: unknown;
  label: string;
  type: string;
  category: NodeCategory;
  icon: string;
  config: Record<string, unknown>;
  status: NodeStatus;
  lastOutput?: unknown;
  inputs: PortDefinition[];
  outputs: PortDefinition[];
}

export interface FlowSummary {
  id: string;
  name: string;
  description: string;
  status: string;
  nodes: number;
  edges: number;
  created_at: string;
  updated_at: string;
}

export interface FlowListResponse {
  flows: FlowSummary[];
  total: number;
}

export interface FlowDetail {
  id: string;
  name: string;
  description: string;
  version: string;
  status: string;
  nodes: unknown[];
  edges: unknown[];
  canvas_nodes: unknown[];
  canvas_edges: unknown[];
  viewport: { x: number; y: number; zoom: number };
  config: unknown;
  created_at: string;
  updated_at: string;
}

export interface CreateFlowRequest {
  name: string;
  description?: string;
}

export interface CreateFlowResponse {
  id: string;
  name: string;
  description: string;
  status: string;
  created_at: string;
}

/** Port type color mapping */
export const PORT_COLORS: Record<PortType, string> = {
  any: "#94A3B8",
  string: "#22C55E",
  number: "#3B82F6",
  boolean: "#F59E0B",
  object: "#8B5CF6",
  array: "#EC4899",
  binary: "#EF4444",
};

/** Node category color mapping */
export const CATEGORY_COLORS: Record<NodeCategory, string> = {
  input: "#22C55E",
  process: "#3B82F6",
  output: "#F59E0B",
  logic: "#8B5CF6",
  data: "#06B6D4",
  ai: "#EC4899",
};
