import { create } from "zustand";

interface UIState {
  sidebarOpen: boolean;
  configPanelOpen: boolean;
  selectedNodeId: string | null;
  theme: "dark" | "light";

  toggleSidebar: () => void;
  openConfigPanel: (nodeId: string) => void;
  closeConfigPanel: () => void;
  setTheme: (theme: "dark" | "light") => void;
}

export const useUIStore = create<UIState>((set) => ({
  sidebarOpen: true,
  configPanelOpen: false,
  selectedNodeId: null,
  theme: "dark",

  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),

  openConfigPanel: (nodeId) =>
    set({ configPanelOpen: true, selectedNodeId: nodeId }),

  closeConfigPanel: () =>
    set({ configPanelOpen: false, selectedNodeId: null }),

  setTheme: (theme) => set({ theme }),
}));
