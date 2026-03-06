import { flowsApi } from "@/api/flows";
import { Header } from "@/components/layout/Header";
import { useEngineSocket } from "@/hooks/useEngineSocket";
import { NODE_DEFINITIONS } from "@/lib/nodeDefinitions";
import { useFlowStore } from "@/stores/flowStore";
import { useUIStore } from "@/stores/uiStore";
import type { Z8NodeData } from "@/types/flow";
import {
  type Edge,
  type Node,
  ReactFlowProvider,
  useReactFlow,
} from "@xyflow/react";
import { useCallback, useEffect } from "react";
import { useParams } from "react-router-dom";
import { FlowCanvas } from "./canvas/FlowCanvas";
import { ConfigPanel } from "./panels/ConfigPanel";
import { ExecutionLog } from "./panels/ExecutionLog";
import { NodePalette } from "./toolbar/NodePalette";

/** Inner component that has access to ReactFlow context for Ctrl+S */
function EditorInner() {
  const { id } = useParams<{ id: string }>();
  const setFlow = useFlowStore((s) => s.setFlow);
  const saveFlow = useFlowStore((s) => s.saveFlow);
  const sidebarOpen = useUIStore((s) => s.sidebarOpen);
  const reactFlow = useReactFlow();

  // Load flow from backend (including saved canvas state)
  // Enrich nodes with inputs/outputs from NODE_DEFINITIONS if missing
  useEffect(() => {
    if (!id) return;
    flowsApi.get(id).then((flow) => {
      const rawNodes = (flow.canvas_nodes ?? []) as Node<Z8NodeData>[];
      const nodes = rawNodes.map((node) => {
        const data = node.data;
        const nodeType =
          data.type ?? ((data as Record<string, unknown>).nodeType as string);
        // If inputs/outputs are missing, look them up from NODE_DEFINITIONS
        if (nodeType && (!data.inputs?.length || !data.outputs?.length)) {
          const def = NODE_DEFINITIONS.find((d) => d.type === nodeType);
          if (def) {
            return {
              ...node,
              data: {
                ...data,
                inputs: data.inputs?.length ? data.inputs : def.inputs,
                outputs: data.outputs?.length ? data.outputs : def.outputs,
                category: data.category ?? def.category,
                icon: data.icon ?? def.icon,
              },
            };
          }
        }
        return node;
      });
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
