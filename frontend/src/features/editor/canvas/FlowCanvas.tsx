import { Z8Node } from "@/features/editor/nodes/Z8Node";
import { NODE_DEFINITIONS, createNodeData } from "@/lib/nodeDefinitions";
import { useFlowStore } from "@/stores/flowStore";
import { useUIStore } from "@/stores/uiStore";
import type { Z8NodeData } from "@/types/flow";
import {
  Background,
  BackgroundVariant,
  Controls,
  type Edge,
  MiniMap,
  type Node,
  ReactFlow,
  type ReactFlowInstance,
} from "@xyflow/react";
import { useCallback, useMemo, useRef } from "react";

const nodeTypes = { z8node: Z8Node };

export function FlowCanvas() {
  const reactFlowRef = useRef<ReactFlowInstance<Node<Z8NodeData>, Edge> | null>(
    null,
  );

  const nodes = useFlowStore((s) => s.nodes);
  const edges = useFlowStore((s) => s.edges);
  const onNodesChange = useFlowStore((s) => s.onNodesChange);
  const onEdgesChange = useFlowStore((s) => s.onEdgesChange);
  const onConnect = useFlowStore((s) => s.onConnect);
  const addNode = useFlowStore((s) => s.addNode);
  const openConfigPanel = useUIStore((s) => s.openConfigPanel);

  const onInit = useCallback(
    (instance: ReactFlowInstance<Node<Z8NodeData>, Edge>) => {
      reactFlowRef.current = instance;
    },
    [],
  );

  // Handle drop from palette
  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  }, []);

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const nodeType = e.dataTransfer.getData("application/z8run-node");
      if (!nodeType || !reactFlowRef.current) return;

      const def = NODE_DEFINITIONS.find((d) => d.type === nodeType);
      if (!def) return;

      const position = reactFlowRef.current.screenToFlowPosition({
        x: e.clientX,
        y: e.clientY,
      });

      addNode(def.type, createNodeData(def), position);
    },
    [addNode],
  );

  // Open config panel on node double-click
  const onNodeDoubleClick = useCallback(
    (_: React.MouseEvent, node: { id: string }) => {
      openConfigPanel(node.id);
    },
    [openConfigPanel],
  );

  const defaultEdgeOptions = useMemo(
    () => ({
      animated: true,
      style: { stroke: "#475569", strokeWidth: 2 },
    }),
    [],
  );

  // Style error edges red + dashed, default edges stay normal
  const styledEdges = useMemo(
    () =>
      edges.map((edge) => {
        if (edge.sourceHandle === "error" || edge.sourceHandle === "reject") {
          return {
            ...edge,
            animated: false,
            style: {
              stroke: "#EF4444",
              strokeWidth: 2,
              strokeDasharray: "6 3",
            },
          };
        }
        return edge;
      }),
    [edges],
  );

  return (
    <div className="flex-1 h-full">
      <ReactFlow
        nodes={nodes}
        edges={styledEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onInit={onInit}
        onDrop={onDrop}
        onDragOver={onDragOver}
        onNodeDoubleClick={onNodeDoubleClick}
        nodeTypes={nodeTypes}
        defaultEdgeOptions={defaultEdgeOptions}
        fitView={nodes.length === 0}
        snapToGrid
        snapGrid={[20, 20]}
        minZoom={0.1}
        maxZoom={4}
        deleteKeyCode={["Delete", "Backspace"]}
        className="bg-canvas"
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={20}
          size={1}
          color="#1e293b"
        />
        <Controls showInteractive={false} />
        <MiniMap
          nodeColor="#3B82F6"
          maskColor="rgb(15, 23, 42, 0.8)"
          pannable
          zoomable
        />
      </ReactFlow>
    </div>
  );
}
