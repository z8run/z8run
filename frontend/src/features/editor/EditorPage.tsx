import { useEffect, useCallback } from "react";
import { useParams } from "react-router-dom";
import { ReactFlowProvider, useReactFlow, type Node, type Edge } from "@xyflow/react";
import { Header } from "@/components/layout/Header";
import { FlowCanvas } from "./canvas/FlowCanvas";
import { NodePalette } from "./toolbar/NodePalette";
import { ConfigPanel } from "./panels/ConfigPanel";
import { ExecutionLog } from "./panels/ExecutionLog";
import { useFlowStore } from "@/stores/flowStore";
import { useUIStore } from "@/stores/uiStore";
import { useEngineSocket } from "@/hooks/useEngineSocket";
import { flowsApi } from "@/api/flows";
import type { Z8NodeData } from "@/types/flow";

/** Inner component that has access to ReactFlow context for Ctrl+S */
function EditorInner() {
  const { id } = useParams<{ id: string }>();
  const setFlow = useFlowStore((s) => s.setFlow);
  const saveFlow = useFlowStore((s) => s.saveFlow);
  const sidebarOpen = useUIStore((s) => s.sidebarOpen);
  const reactFlow = useReactFlow();

  // Load flow from backend (including saved canvas state)
  useEffect(() => {
    if (!id) return;
    flowsApi.get(id).then((flow) => {
      const nodes = (flow.canvas_nodes ?? []) as Node<Z8NodeData>[];
      const edges = (flow.canvas_edges ?? []) as Edge[];
      setFlow(flow.id, flow.name, nodes, edges);
    });
  }, [id, setFlow]);

  // Ctrl+S / Cmd+S keyboard shortcut
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        const { dirty } = useFlowStore.getState();
        if (dirty) {
          saveFlow(reactFlow.getViewport());
        }
      }
    },
    [saveFlow, reactFlow],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // Connect to WebSocket for engine events
  useEngineSocket();

  return (
    <div className="h-screen flex flex-col">
      <Header />
      <div className="flex flex-1 overflow-hidden">
        {sidebarOpen && <NodePalette />}
        <div className="flex-1 flex flex-col">
          <FlowCanvas />
          <ExecutionLog />
        </div>
        <ConfigPanel />
      </div>
    </div>
  );
}

export function EditorPage() {
  return (
    <ReactFlowProvider>
      <EditorInner />
    </ReactFlowProvider>
  );
}
