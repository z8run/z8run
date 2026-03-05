import { create } from "zustand";
import { flowsApi } from "@/api/flows";
import type { FlowSummary } from "@/types/flow";

interface FlowListState {
  flows: FlowSummary[];
  loading: boolean;
  error: string | null;

  fetchFlows: () => Promise<void>;
  createFlow: (name: string, description?: string) => Promise<string>;
  deleteFlow: (id: string) => Promise<void>;
}

export const useFlowListStore = create<FlowListState>((set, get) => ({
  flows: [],
  loading: false,
  error: null,

  fetchFlows: async () => {
    set({ loading: true, error: null });
    try {
      const res = await flowsApi.list();
      set({ flows: res.flows, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  createFlow: async (name, description) => {
    const res = await flowsApi.create({ name, description });
    await get().fetchFlows();
    return res.id;
  },

  deleteFlow: async (id) => {
    await flowsApi.delete(id);
    set({ flows: get().flows.filter((f) => f.id !== id) });
  },
}));
