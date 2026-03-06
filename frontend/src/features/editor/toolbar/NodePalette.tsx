import {
  NODE_CATEGORIES,
  NODE_DEFINITIONS,
  type NodeDefinition,
} from "@/lib/nodeDefinitions";
import { CATEGORY_COLORS, type NodeCategory } from "@/types/flow";
import { Search } from "lucide-react";
import {
  AlignLeft,
  Bot,
  Braces,
  Brain,
  Bug,
  Clock,
  Code,
  Database,
  FileText,
  Filter,
  Fingerprint,
  GitBranch,
  Globe,
  Image,
  Radio,
  Scissors,
  Send,
  Tags,
  Timer,
  Webhook,
} from "lucide-react";
import { useCallback, useState } from "react";

const ICON_MAP: Record<
  string,
  React.ComponentType<{ size?: number; className?: string }>
> = {
  Globe,
  Clock,
  Webhook,
  Code,
  Braces,
  Filter,
  Bug,
  Send,
  GitBranch,
  Timer,
  Database,
  Radio,
  Brain,
  Fingerprint,
  Tags,
  FileText,
  Scissors,
  AlignLeft,
  Bot,
  Image,
};

function PaletteNode({ def }: { def: NodeDefinition }) {
  const Icon = ICON_MAP[def.icon];
  const color = CATEGORY_COLORS[def.category];

  const onDragStart = useCallback(
    (e: React.DragEvent) => {
      e.dataTransfer.setData("application/z8run-node", def.type);
      e.dataTransfer.effectAllowed = "move";
    },
    [def.type],
  );

  return (
    <div
      draggable
      onDragStart={onDragStart}
      className="flex items-center gap-2.5 px-3 py-2 rounded-md cursor-grab
        hover:bg-slate-700/50 transition-colors active:cursor-grabbing"
    >
      <div
        className="w-7 h-7 rounded flex items-center justify-center flex-shrink-0"
        style={{ backgroundColor: `${color}20` }}
      >
        {Icon && <Icon size={14} className="text-slate-300" />}
      </div>
      <div className="min-w-0">
        <div className="text-xs font-medium text-slate-200 truncate">
          {def.label}
        </div>
        <div className="text-[10px] text-slate-500 truncate">
          {def.description}
        </div>
      </div>
    </div>
  );
}

export function NodePalette() {
  const [search, setSearch] = useState("");

  const filtered = NODE_DEFINITIONS.filter(
    (d) =>
      d.label.toLowerCase().includes(search.toLowerCase()) ||
      d.type.toLowerCase().includes(search.toLowerCase()) ||
      d.description.toLowerCase().includes(search.toLowerCase()),
  );

  const grouped = NODE_CATEGORIES.map((cat) => ({
    ...cat,
    nodes: filtered.filter((d) => d.category === cat.id),
  })).filter((g) => g.nodes.length > 0);

  return (
    <div className="w-[260px] bg-slate-900 border-r border-slate-700 flex flex-col h-full">
      {/* Header */}
      <div className="p-3 border-b border-slate-700">
        <div className="flex items-center gap-2 mb-3">
          <div className="w-6 h-6 rounded bg-z8-600 flex items-center justify-center">
            <span className="text-[10px] font-bold text-white">z8</span>
          </div>
          <span className="text-sm font-semibold text-slate-200">Nodes</span>
        </div>
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-500"
          />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search nodes..."
            className="w-full bg-slate-800 border border-slate-700 rounded-md pl-8 pr-3 py-1.5
              text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-z8-500
              transition-colors"
          />
        </div>
      </div>

      {/* Node list */}
      <div className="flex-1 overflow-y-auto p-2 space-y-3">
        {grouped.map((group) => (
          <div key={group.id}>
            <div className="flex items-center gap-2 px-2 py-1 mb-1">
              <div
                className="w-2 h-2 rounded-full"
                style={{
                  backgroundColor: CATEGORY_COLORS[group.id as NodeCategory],
                }}
              />
              <span className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
                {group.label}
              </span>
            </div>
            {group.nodes.map((def) => (
              <PaletteNode key={def.type} def={def} />
            ))}
          </div>
        ))}
        {grouped.length === 0 && (
          <div className="text-center text-xs text-slate-600 py-8">
            No nodes found
          </div>
        )}
      </div>
    </div>
  );
}
