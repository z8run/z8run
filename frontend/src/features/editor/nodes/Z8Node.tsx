import { memo, useCallback } from "react";
import { Handle, Position, type NodeProps, useReactFlow } from "@xyflow/react";
import type { Z8NodeData } from "@/types/flow";
import { CATEGORY_COLORS, PORT_COLORS } from "@/types/flow";
import { useUIStore } from "@/stores/uiStore";
import {
  Globe, Clock, Webhook, Code, Braces, Filter,
  Bug, Send, GitBranch, Timer, Database, X,
} from "lucide-react";

const ICON_MAP: Record<string, React.ComponentType<{ size?: number }>> = {
  Globe, Clock, Webhook, Code, Braces, Filter,
  Bug, Send, GitBranch, Timer, Database,
};

const STATUS_STYLES: Record<string, string> = {
  idle: "border-slate-600",
  running: "border-blue-400 shadow-blue-500/30 shadow-lg animate-pulse",
  success: "border-green-400 shadow-green-500/20 shadow-md",
  error: "border-red-500 shadow-red-500/20 shadow-md",
  disabled: "border-slate-700 opacity-50",
};

function Z8NodeComponent({ id, data, selected }: NodeProps) {
  const nodeData = data as unknown as Z8NodeData;
  const openConfigPanel = useUIStore((s) => s.openConfigPanel);
  const { deleteElements } = useReactFlow();
  const Icon = ICON_MAP[nodeData.icon];
  const categoryColor = CATEGORY_COLORS[nodeData.category] ?? "#6366f1";
  const status = nodeData.status ?? "idle";
  const statusStyle = STATUS_STYLES[status] || STATUS_STYLES.idle;
  const inputs = nodeData.inputs ?? [];
  const outputs = nodeData.outputs ?? [];

  const handleDelete = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      deleteElements({ nodes: [{ id }] });
    },
    [id, deleteElements],
  );

  return (
    <div
      className={`
        relative bg-slate-800 rounded-lg border-2 min-w-[180px] cursor-pointer
        transition-all duration-150 group/node
        ${statusStyle}
        ${selected ? "ring-2 ring-z8-500 ring-offset-1 ring-offset-slate-950" : ""}
      `}
      onDoubleClick={() => openConfigPanel(id)}
    >
      {/* Delete button */}
      <button
        type="button"
        onClick={handleDelete}
        className="absolute -top-2 -right-2 w-5 h-5 rounded-full bg-red-600 hover:bg-red-500
          flex items-center justify-center opacity-0 group-hover/node:opacity-100
          transition-opacity z-10 shadow-md"
      >
        <X size={12} className="text-white" />
      </button>

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 rounded-t-md"
        style={{ backgroundColor: `${categoryColor}15` }}
      >
        <div
          className="w-6 h-6 rounded flex items-center justify-center"
          style={{ backgroundColor: `${categoryColor}30` }}
        >
          {Icon && <Icon size={14} />}
        </div>
        <span className="text-xs font-medium text-slate-200 truncate">
          {nodeData.label}
        </span>
      </div>

      {/* Type label */}
      <div className="px-3 py-2">
        <span className="text-[10px] text-slate-500 font-mono">
          {nodeData.type ?? (nodeData as Record<string, unknown>).nodeType ?? "node"}
        </span>
      </div>

      {/* Input handles */}
      {inputs.map((port, i) => {
        const top = 36 + i * 24;
        return (
          <Handle
            key={port.id}
            type="target"
            position={Position.Left}
            id={port.id}
            style={{
              top,
              background: PORT_COLORS[port.type],
              width: 10,
              height: 10,
              border: "2px solid #1e293b",
            }}
          />
        );
      })}

      {/* Output handles */}
      {outputs.map((port, i) => {
        const top = 36 + i * 24;
        return (
          <Handle
            key={port.id}
            type="source"
            position={Position.Right}
            id={port.id}
            style={{
              top,
              background: PORT_COLORS[port.type],
              width: 10,
              height: 10,
              border: "2px solid #1e293b",
            }}
          />
        );
      })}
    </div>
  );
}

export const Z8Node = memo(Z8NodeComponent);
