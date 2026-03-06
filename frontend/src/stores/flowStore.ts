import { type SaveFlowRequest, flowsApi } from "@/api/flows";
import type { NodeStatus, Z8NodeData } from "@/types/flow";
import {
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type XYPosition,
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
} from "@xyflow/react";
import { create } from "zustand";

interface FlowState {
  // Current flow metadata
  flowId: string | null;
  flowName: string;
  saving: boolean;
  dirty: boolean;

  // React Flow state
  nodes: Node<Z8NodeData>[];
  edges: Edge[];

  // Actions
  setFlow: (
    id: string,
    name: string,
    nodes: Node<Z8NodeData>[],
    edges: Edge[],
  ) => void;
  onNodesChange: (changes: NodeChange<Node<Z8NodeData>>[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  addNode: (type: string, data: Z8NodeData, position: XYPosition) => void;
  updateNodeData: (id: string, data: Partial<Z8NodeData>) => void;
  setNodeStatus: (id: string, status: NodeStatus) => void;
  resetAllNodeStatus: () => void;
  setFlowName: (name: string) => void;
  removeSelected: () => void;
  clear: () => void;
  saveFlow: (viewport: { x: number; y: number; zoom: number }) => Promise<void>;
}

let nodeIdCounter = 0;

export const useFlowStore = create<FlowState>((set, get) => ({
  flowId: null,
  flowName: "Untitled Flow",
  saving: false,
  dirty: false,
  nodes: [],
  edges: [],

  setFlow: (id, name, nodes, edges) =>
    set({ flowId: id, flowName: name, nodes, edges, dirty: false }),

  onNodesChange: (changes) =>
    set({ nodes: applyNodeChanges(changes, get().nodes), dirty: true }),

  onEdgesChange: (changes) =>
    set({ edges: applyEdgeChanges(changes, get().edges), dirty: true }),

  onConnect: (connection) =>
    set({
      edges: addEdge({ ...connection, animated: true }, get().edges),
      dirty: true,
    }),

  addNode: (_type, data, position) => {
    nodeIdCounter++;
    const id = `node_${Date.now()}_${nodeIdCounter}`;
    const newNode: Node<Z8NodeData> = {
      id,
      type: "z8node",
      position,
      data,
    };
    set({ nodes: [...get().nodes, newNode], dirty: true });
  },

  setFlowName: (name) => set({ flowName: name, dirty: true }),

  updateNodeData: (id, data) =>
    set({
      nodes: get().nodes.map((node) =>
        node.id === id ? { ...node, data: { ...node.data, ...data } } : node,
      ),
      dirty: true,
    }),

  setNodeStatus: (id, status: NodeStatus) =>
    set({
      nodes: get().nodes.map((node) =>
        node.id === id ? { ...node, data: { ...node.data, status } } : node,
      ),
    }),

  resetAllNodeStatus: () =>
    set({
      nodes: get().nodes.map((node) => ({
        ...node,
        data: { ...node.data, status: "idle" as NodeStatus },
      })),
    }),

  removeSelected: () =>
    set({
      nodes: get().nodes.filter((n) => !n.selected),
      edges: get().edges.filter((e) => !e.selected),
      dirty: true,
    }),

  clear: () =>
    set({
      flowId: null,
      flowName: "Untitled Flow",
      nodes: [],
      edges: [],
      dirty: false,
    }),

  saveFlow: async (viewport) => {
    const { flowId, flowName, nodes, edges } = get();
    if (!flowId) return;

    set({ saving: true });
    try {
      const payload: SaveFlowRequest = {
        name: flowName,
        canvas_nodes: nodes,
        canvas_edges: edges,
        viewport,
      };
      await flowsApi.update(flowId, payload);
      set({ dirty: false });
    } finally {
      set({ saving: false });
    }
  },
}));
