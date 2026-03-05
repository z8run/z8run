import { X, Settings, Zap, AlertCircle, CheckCircle2, Loader2, Circle, Plus, Trash2 } from "lucide-react";
import { useUIStore } from "@/stores/uiStore";
import { useFlowStore } from "@/stores/flowStore";
import type { Z8NodeData, NodeStatus } from "@/types/flow";
import { CATEGORY_COLORS, PORT_COLORS } from "@/types/flow";

const STATUS_BADGES: Record<NodeStatus, { label: string; color: string; Icon: React.ComponentType<{ size?: number; className?: string }> }> = {
  idle: { label: "Idle", color: "text-slate-500", Icon: Circle },
  running: { label: "Running", color: "text-blue-400", Icon: Loader2 },
  success: { label: "Success", color: "text-green-400", Icon: CheckCircle2 },
  error: { label: "Error", color: "text-red-400", Icon: AlertCircle },
  disabled: { label: "Disabled", color: "text-slate-600", Icon: Circle },
};

/** Convert camelCase or snake_case key to a human-friendly label */
function humanizeKey(key: string): string {
  return key
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

/** HTTP methods for dropdown */
const HTTP_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

/** Database types */
const DB_TYPES = [
  { value: "postgres", label: "PostgreSQL" },
  { value: "mysql", label: "MySQL" },
  { value: "sqlite", label: "SQLite" },
  { value: "mssql", label: "SQL Server" },
];

/** Common HTTP status codes for dropdown */
const HTTP_STATUS_CODES = [
  { value: 200, label: "200 — OK" },
  { value: 201, label: "201 — Created" },
  { value: 204, label: "204 — No Content" },
  { value: 301, label: "301 — Moved" },
  { value: 400, label: "400 — Bad Request" },
  { value: 401, label: "401 — Unauthorized" },
  { value: 403, label: "403 — Forbidden" },
  { value: 404, label: "404 — Not Found" },
  { value: 500, label: "500 — Server Error" },
];

const selectClass = `w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
  text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors`;
const inputClass = `w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
  text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500 transition-colors`;

/** Render a smart config field based on key name and node type */
function SmartConfigField({
  fieldKey,
  value,
  nodeType,
  onChange,
}: {
  fieldKey: string;
  value: unknown;
  nodeType: string;
  onChange: (val: unknown) => void;
}) {
  // Method dropdown for HTTP nodes
  if (fieldKey === "method") {
    return (
      <select
        value={String(value ?? "GET")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        {HTTP_METHODS.map((m) => (
          <option key={m} value={m}>{m}</option>
        ))}
      </select>
    );
  }

  // Status code dropdown for http-out
  if (fieldKey === "statusCode") {
    const current = Number(value ?? 200);
    return (
      <select
        value={current}
        onChange={(e) => onChange(Number(e.target.value))}
        className={selectClass}
      >
        {HTTP_STATUS_CODES.map((s) => (
          <option key={s.value} value={s.value}>{s.label}</option>
        ))}
        {/* If current value isn't in the list, show it too */}
        {!HTTP_STATUS_CODES.some((s) => s.value === current) && (
          <option value={current}>{current}</option>
        )}
      </select>
    );
  }

  // URL field with placeholder
  if (fieldKey === "url") {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="https://api.example.com/endpoint"
        className={inputClass}
      />
    );
  }

  // Timeout with ms suffix
  if (fieldKey === "timeout") {
    return (
      <div className="flex items-center gap-1.5">
        <input
          type="number"
          value={Number(value ?? 5000)}
          onChange={(e) => onChange(Number(e.target.value))}
          className={`${inputClass} flex-1`}
          min={100}
          step={500}
        />
        <span className="text-[10px] text-slate-500 shrink-0">ms</span>
      </div>
    );
  }

  // Action dropdown for JSON Transform
  if (fieldKey === "action" && nodeType === "json") {
    return (
      <select
        value={String(value ?? "parse")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="parse">Parse (string → object)</option>
        <option value="stringify">Stringify (object → string)</option>
        <option value="extract">Extract (dot-notation path)</option>
      </select>
    );
  }

  // Database type dropdown
  if (fieldKey === "dbType" && nodeType === "database") {
    return (
      <select
        value={String(value ?? "postgres")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        {DB_TYPES.map((db) => (
          <option key={db.value} value={db.value}>{db.label}</option>
        ))}
      </select>
    );
  }

  // Database host
  if (fieldKey === "host" && nodeType === "database") {
    return (
      <input
        type="text"
        value={String(value ?? "localhost")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="localhost o IP del servidor"
        className={inputClass}
      />
    );
  }

  // Database port
  if (fieldKey === "port" && nodeType === "database") {
    return (
      <input
        type="number"
        value={Number(value ?? 5432)}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputClass}
        min={0}
        max={65535}
      />
    );
  }

  // Database name
  if (fieldKey === "database" && nodeType === "database") {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="nombre_de_la_base_de_datos"
        className={inputClass}
      />
    );
  }

  // Database user
  if (fieldKey === "user" && nodeType === "database") {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="usuario"
        className={inputClass}
      />
    );
  }

  // Database password
  if (fieldKey === "password" && nodeType === "database") {
    return (
      <input
        type="password"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="••••••••"
        className={inputClass}
      />
    );
  }

  // Query textarea for database
  if (fieldKey === "query" && nodeType === "database") {
    return (
      <textarea
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        rows={4}
        placeholder="SELECT * FROM users WHERE id = $1"
        className={`${inputClass} resize-y`}
        spellCheck={false}
      />
    );
  }

  // Unit dropdown for delay
  if (fieldKey === "unit") {
    return (
      <select
        value={String(value ?? "ms")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="ms">Milliseconds</option>
        <option value="s">Seconds</option>
        <option value="m">Minutes</option>
      </select>
    );
  }

  // --- AI NODES: shared fields ---
  const AI_NODE_TYPES = ["llm", "embeddings", "classifier", "structured-output", "summarizer", "ai-agent", "image-gen"];

  // AI Provider dropdown
  if (fieldKey === "provider" && AI_NODE_TYPES.includes(nodeType)) {
    const providers = nodeType === "image-gen"
      ? [{ value: "openai", label: "OpenAI (DALL-E)" }, { value: "stability", label: "Stability AI" }]
      : [{ value: "openai", label: "OpenAI" }, { value: "anthropic", label: "Anthropic" }, { value: "ollama", label: "Ollama (Local)" }];
    return (
      <select
        value={String(value ?? "openai")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        {providers.map((p) => (
          <option key={p.value} value={p.value}>{p.label}</option>
        ))}
      </select>
    );
  }

  // AI Model input
  if (fieldKey === "model" && AI_NODE_TYPES.includes(nodeType)) {
    const placeholders: Record<string, string> = {
      "openai-llm": "gpt-4o-mini",
      "openai-embeddings": "text-embedding-3-small",
      "openai-structured-output": "gpt-4o-mini",
      "openai-summarizer": "gpt-4o-mini",
      "openai-ai-agent": "gpt-4o",
      "openai-image-gen": "dall-e-3",
      "anthropic-llm": "claude-sonnet-4-20250514",
      "anthropic-ai-agent": "claude-sonnet-4-20250514",
      "ollama-llm": "llama3",
    };
    const placeholder = placeholders[`${String(value ?? "openai").split("-")[0] || "openai"}-${nodeType}`] || "Model name";
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={inputClass}
      />
    );
  }

  // API Key (password field)
  if (fieldKey === "apiKey" && AI_NODE_TYPES.includes(nodeType)) {
    return (
      <input
        type="password"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="sk-..."
        className={inputClass}
      />
    );
  }

  // Base URL
  if (fieldKey === "baseUrl" && AI_NODE_TYPES.includes(nodeType)) {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Custom endpoint (optional)"
        className={inputClass}
      />
    );
  }

  // System Prompt (textarea) — LLM, AI Agent
  if (fieldKey === "systemPrompt" && ["llm", "ai-agent"].includes(nodeType)) {
    return (
      <textarea
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="You are a helpful assistant..."
        rows={4}
        className={`${inputClass} resize-y`}
        spellCheck={false}
      />
    );
  }

  // Temperature — LLM, Classifier, AI Agent
  if (fieldKey === "temperature" && ["llm", "classifier", "ai-agent"].includes(nodeType)) {
    return (
      <input
        type="number"
        value={Number(value ?? 0.7)}
        onChange={(e) => onChange(Number(e.target.value))}
        step="0.1"
        min="0"
        max="2"
        className={inputClass}
      />
    );
  }

  // Max Tokens — LLM, Classifier
  if (fieldKey === "maxTokens" && ["llm", "classifier"].includes(nodeType)) {
    return (
      <input
        type="number"
        value={Number(value ?? 1024)}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputClass}
        min={1}
      />
    );
  }

  // Categories (classifier)
  if (fieldKey === "categories" && nodeType === "classifier") {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="positive, negative, neutral"
        className={inputClass}
      />
    );
  }

  // Context (classifier)
  if (fieldKey === "context" && nodeType === "classifier") {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="What are you classifying?"
        className={inputClass}
      />
    );
  }

  // --- PROMPT TEMPLATE ---
  if (fieldKey === "template" && nodeType === "prompt-template") {
    return (
      <textarea
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Hello {{name}}, you asked: {{prompt}}"
        rows={5}
        className={`${inputClass} resize-y`}
        spellCheck={false}
      />
    );
  }

  if (fieldKey === "strictMode" && nodeType === "prompt-template") {
    return null; // handled by generic boolean renderer
  }

  // --- TEXT SPLITTER ---
  if (fieldKey === "strategy" && nodeType === "text-splitter") {
    return (
      <select
        value={String(value ?? "fixed")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="fixed">Fixed size</option>
        <option value="sentences">By sentences</option>
        <option value="paragraphs">By paragraphs</option>
        <option value="tokens">By tokens (approx)</option>
      </select>
    );
  }

  if (fieldKey === "chunkSize" && nodeType === "text-splitter") {
    return (
      <div className="flex items-center gap-1.5">
        <input
          type="number"
          value={Number(value ?? 512)}
          onChange={(e) => onChange(Number(e.target.value))}
          className={`${inputClass} flex-1`}
          min={50}
          step={50}
        />
        <span className="text-[10px] text-slate-500 shrink-0">chars</span>
      </div>
    );
  }

  if (fieldKey === "overlap" && nodeType === "text-splitter") {
    return (
      <div className="flex items-center gap-1.5">
        <input
          type="number"
          value={Number(value ?? 50)}
          onChange={(e) => onChange(Number(e.target.value))}
          className={`${inputClass} flex-1`}
          min={0}
          step={10}
        />
        <span className="text-[10px] text-slate-500 shrink-0">chars</span>
      </div>
    );
  }

  // --- VECTOR STORE ---
  if (fieldKey === "action" && nodeType === "vector-store") {
    return (
      <select
        value={String(value ?? "store")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="store">Store embedding</option>
        <option value="search">Search similar</option>
        <option value="delete">Delete entry</option>
        <option value="clear">Clear collection</option>
      </select>
    );
  }

  if (fieldKey === "collection" && nodeType === "vector-store") {
    return (
      <input
        type="text"
        value={String(value ?? "default")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Collection name"
        className={inputClass}
      />
    );
  }

  if (fieldKey === "topK" && nodeType === "vector-store") {
    return (
      <input
        type="number"
        value={Number(value ?? 5)}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputClass}
        min={1}
        max={100}
      />
    );
  }

  if (fieldKey === "minScore" && nodeType === "vector-store") {
    return (
      <input
        type="number"
        value={Number(value ?? 0.0)}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputClass}
        min={0}
        max={1}
        step={0.05}
      />
    );
  }

  // --- STRUCTURED OUTPUT ---
  if (fieldKey === "schema" && nodeType === "structured-output") {
    return (
      <textarea
        value={String(value ?? "{}")}
        onChange={(e) => onChange(e.target.value)}
        placeholder='{"type":"object","properties":{"name":{"type":"string"}}}'
        rows={5}
        className={`${inputClass} resize-y`}
        spellCheck={false}
      />
    );
  }

  if (fieldKey === "retries" && nodeType === "structured-output") {
    return (
      <input
        type="number"
        value={Number(value ?? 2)}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputClass}
        min={0}
        max={5}
      />
    );
  }

  // --- SUMMARIZER ---
  if (fieldKey === "strategy" && nodeType === "summarizer") {
    return (
      <select
        value={String(value ?? "simple")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="simple">Simple (full text)</option>
        <option value="map-reduce">Map-Reduce (long docs)</option>
      </select>
    );
  }

  if (fieldKey === "maxLength" && nodeType === "summarizer") {
    return (
      <div className="flex items-center gap-1.5">
        <input
          type="number"
          value={Number(value ?? 200)}
          onChange={(e) => onChange(Number(e.target.value))}
          className={`${inputClass} flex-1`}
          min={50}
          step={50}
        />
        <span className="text-[10px] text-slate-500 shrink-0">words</span>
      </div>
    );
  }

  if (fieldKey === "language" && nodeType === "summarizer") {
    return (
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Output language (empty = same as input)"
        className={inputClass}
      />
    );
  }

  // --- AI AGENT ---
  if (fieldKey === "tools" && nodeType === "ai-agent") {
    return (
      <textarea
        value={String(value ?? "[]")}
        onChange={(e) => onChange(e.target.value)}
        placeholder='[{"name":"search","description":"Search the web","parameters":{"type":"object","properties":{"query":{"type":"string"}}}}]'
        rows={6}
        className={`${inputClass} resize-y`}
        spellCheck={false}
      />
    );
  }

  if (fieldKey === "maxIterations" && nodeType === "ai-agent") {
    return (
      <input
        type="number"
        value={Number(value ?? 5)}
        onChange={(e) => onChange(Number(e.target.value))}
        className={inputClass}
        min={1}
        max={20}
      />
    );
  }

  // --- IMAGE GEN ---
  if (fieldKey === "size" && nodeType === "image-gen") {
    return (
      <select
        value={String(value ?? "1024x1024")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="1024x1024">1024 x 1024 (Square)</option>
        <option value="1024x1792">1024 x 1792 (Portrait)</option>
        <option value="1792x1024">1792 x 1024 (Landscape)</option>
      </select>
    );
  }

  if (fieldKey === "quality" && nodeType === "image-gen") {
    return (
      <select
        value={String(value ?? "standard")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="standard">Standard</option>
        <option value="hd">HD</option>
      </select>
    );
  }

  if (fieldKey === "style" && nodeType === "image-gen") {
    return (
      <select
        value={String(value ?? "vivid")}
        onChange={(e) => onChange(e.target.value)}
        className={selectClass}
      >
        <option value="vivid">Vivid</option>
        <option value="natural">Natural</option>
      </select>
    );
  }

  // No smart field
  return null;
}

/** Check if a key has a smart field renderer */
function hasSmartField(key: string, nodeType: string): boolean {
  const AI_NODES = ["llm", "embeddings", "classifier", "structured-output", "summarizer", "ai-agent", "image-gen"];
  return ["method", "statusCode", "url", "timeout"].includes(key)
    || (key === "action" && nodeType === "json")
    || key === "unit"
    || (nodeType === "database" && ["dbType", "host", "port", "database", "user", "password", "query"].includes(key))
    || (AI_NODES.includes(nodeType) &&
        ["provider", "model", "apiKey", "baseUrl", "systemPrompt", "temperature", "maxTokens", "categories", "context"].includes(key))
    || (nodeType === "prompt-template" && ["template"].includes(key))
    || (nodeType === "text-splitter" && ["strategy", "chunkSize", "overlap"].includes(key))
    || (nodeType === "vector-store" && ["action", "collection", "topK", "minScore"].includes(key))
    || (nodeType === "structured-output" && ["schema", "retries"].includes(key))
    || (nodeType === "summarizer" && ["strategy", "maxLength", "language"].includes(key))
    || (nodeType === "ai-agent" && ["tools", "maxIterations"].includes(key))
    || (nodeType === "image-gen" && ["size", "quality", "style"].includes(key));
}

/** Available switch operators with human-friendly labels */
const SWITCH_OPERATORS: { value: string; label: string }[] = [
  { value: "eq", label: "== (equals)" },
  { value: "neq", label: "!= (not equal)" },
  { value: "gt", label: "> (greater)" },
  { value: "lt", label: "< (less)" },
  { value: "gte", label: ">= (greater or equal)" },
  { value: "lte", label: "<= (less or equal)" },
  { value: "contains", label: "contains" },
  { value: "regex", label: "matches regex" },
  { value: "empty", label: "is empty" },
  { value: "notempty", label: "is not empty" },
  { value: "true", label: "is truthy" },
  { value: "false", label: "is falsy" },
];

interface SwitchRule {
  type: string;
  value?: string;
  port: string;
}

/** Inline rules editor for Switch nodes */
function SwitchRulesEditor({
  rules,
  outputs,
  onChange,
}: {
  rules: SwitchRule[];
  outputs: { id: string; name: string }[];
  onChange: (rules: SwitchRule[]) => void;
}) {
  const updateRule = (index: number, field: keyof SwitchRule, val: string) => {
    const updated = rules.map((r, i) => (i === index ? { ...r, [field]: val } : r));
    onChange(updated);
  };

  const addRule = () => {
    const nextPort = outputs.find(
      (o) => o.id !== "default" && !rules.some((r) => r.port === o.id)
    );
    onChange([
      ...rules,
      { type: "eq", value: "", port: nextPort?.id ?? outputs[0]?.id ?? "out1" },
    ]);
  };

  const removeRule = (index: number) => {
    onChange(rules.filter((_, i) => i !== index));
  };

  // Operators that don't need a value input
  const noValueOps = new Set(["empty", "notempty", "true", "false"]);

  return (
    <div className="space-y-2">
      {rules.map((rule, i) => (
        <div key={i} className="bg-slate-800/60 rounded-md p-2 space-y-1.5 border border-slate-700/50">
          <div className="flex items-center gap-1.5">
            <span className="text-[10px] text-slate-500 shrink-0 w-4">{i + 1}.</span>
            <select
              value={rule.type}
              onChange={(e) => updateRule(i, "type", e.target.value)}
              className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1
                text-[11px] text-slate-200 focus:outline-none focus:border-z8-500"
            >
              {SWITCH_OPERATORS.map((op) => (
                <option key={op.value} value={op.value}>
                  {op.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={() => removeRule(i)}
              className="p-1 hover:bg-slate-700 rounded transition-colors"
            >
              <Trash2 size={11} className="text-slate-600 hover:text-red-400" />
            </button>
          </div>
          <div className="flex items-center gap-1.5 ml-4">
            {!noValueOps.has(rule.type) && (
              <input
                type="text"
                value={rule.value ?? ""}
                onChange={(e) => updateRule(i, "value", e.target.value)}
                placeholder="value"
                className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1
                  text-[11px] text-slate-200 font-mono focus:outline-none focus:border-z8-500"
              />
            )}
            <span className="text-[10px] text-slate-600 shrink-0">→</span>
            <select
              value={rule.port}
              onChange={(e) => updateRule(i, "port", e.target.value)}
              className="w-24 bg-slate-800 border border-slate-700 rounded px-2 py-1
                text-[11px] text-slate-200 focus:outline-none focus:border-z8-500"
            >
              {outputs
                .filter((o) => o.id !== "default")
                .map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
            </select>
          </div>
        </div>
      ))}
      <button
        type="button"
        onClick={addRule}
        className="flex items-center gap-1.5 text-[11px] text-slate-500 hover:text-z8-400
          px-2 py-1 hover:bg-slate-800 rounded transition-colors w-full"
      >
        <Plus size={12} />
        <span>Add rule</span>
      </button>
    </div>
  );
}

export function ConfigPanel() {
  const selectedNodeId = useUIStore((s) => s.selectedNodeId);
  const configPanelOpen = useUIStore((s) => s.configPanelOpen);
  const closeConfigPanel = useUIStore((s) => s.closeConfigPanel);
  const nodes = useFlowStore((s) => s.nodes);
  const updateNodeData = useFlowStore((s) => s.updateNodeData);

  if (!configPanelOpen || !selectedNodeId) return null;

  const node = nodes.find((n) => n.id === selectedNodeId);
  if (!node) return null;

  const data = node.data as unknown as Z8NodeData;
  const color = CATEGORY_COLORS[data.category] ?? "#6366f1";
  const config = data.config ?? {};
  const inputs = data.inputs ?? [];
  const outputs = data.outputs ?? [];
  const status: NodeStatus = data.status ?? "idle";
  const statusBadge = STATUS_BADGES[status] ?? STATUS_BADGES.idle;
  const nodeType = String(data.nodeType ?? data.type ?? "unknown");

  const handleConfigChange = (key: string, value: unknown) => {
    updateNodeData(selectedNodeId, {
      config: { ...config, [key]: value },
    } as Partial<Z8NodeData>);
  };

  return (
    <div className="w-[340px] bg-slate-900 border-l border-slate-700 flex flex-col h-full">
      {/* Header with color accent */}
      <div className="border-b border-slate-700">
        <div
          className="h-1 w-full"
          style={{ backgroundColor: color }}
        />
        <div className="flex items-center justify-between p-3">
          <div className="flex items-center gap-2.5">
            <div
              className="w-8 h-8 rounded-lg flex items-center justify-center"
              style={{ backgroundColor: `${color}20`, border: `1px solid ${color}40` }}
            >
              <Zap size={16} style={{ color }} />
            </div>
            <div>
              <div className="text-sm font-medium text-slate-200">{data.label}</div>
              <div className="text-[10px] text-slate-500 font-mono">{nodeType}</div>
            </div>
          </div>
          <button
            type="button"
            onClick={closeConfigPanel}
            className="p-1.5 hover:bg-slate-700 rounded-lg transition-colors"
          >
            <X size={16} className="text-slate-400" />
          </button>
        </div>

        {/* Status badge */}
        <div className="px-3 pb-2.5 flex items-center gap-2">
          <div className={`flex items-center gap-1.5 text-[11px] ${statusBadge.color}`}>
            <statusBadge.Icon size={12} className={status === "running" ? "animate-spin" : ""} />
            <span>{statusBadge.label}</span>
          </div>
        </div>
      </div>

      {/* Config form */}
      <div className="flex-1 overflow-y-auto p-3 space-y-4">
        {/* Node Name */}
        <div>
          <label className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider block mb-1">
            Node Name
          </label>
          <input
            type="text"
            value={data.label}
            onChange={(e) =>
              updateNodeData(selectedNodeId, { label: e.target.value } as Partial<Z8NodeData>)
            }
            className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
              text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors"
          />
        </div>

        {/* Configuration section */}
        {Object.keys(config).length > 0 && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <Settings size={12} className="text-slate-500" />
              <span className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
                Configuration
              </span>
            </div>
            <div className="space-y-2.5">
              {Object.entries(config).map(([key, value]) => {
                // Switch rules get a dedicated editor
                if (key === "rules" && nodeType === "switch" && Array.isArray(value)) {
                  return (
                    <div key={key}>
                      <label className="text-[10px] font-medium text-slate-400 block mb-1.5">
                        Rules
                      </label>
                      <SwitchRulesEditor
                        rules={value as SwitchRule[]}
                        outputs={outputs}
                        onChange={(newRules) => handleConfigChange("rules", newRules)}
                      />
                    </div>
                  );
                }

                return (
                <div key={key}>
                  <label className="text-[10px] font-medium text-slate-400 block mb-1">
                    {humanizeKey(key)}
                  </label>
                  {hasSmartField(key, nodeType) ? (
                    <SmartConfigField
                      fieldKey={key}
                      value={value}
                      nodeType={nodeType}
                      onChange={(val) => handleConfigChange(key, val)}
                    />
                  ) : typeof value === "string" && (key === "code" || value.length > 50) ? (
                    <textarea
                      value={String(value)}
                      onChange={(e) => handleConfigChange(key, e.target.value)}
                      rows={key === "code" ? 6 : 3}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                        transition-colors resize-y"
                      spellCheck={false}
                    />
                  ) : typeof value === "number" ? (
                    <input
                      type="number"
                      value={Number(value)}
                      onChange={(e) => handleConfigChange(key, Number(e.target.value))}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 focus:outline-none focus:border-z8-500 transition-colors"
                    />
                  ) : typeof value === "boolean" ? (
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => handleConfigChange(key, !value)}
                        className={`
                          w-9 h-5 rounded-full transition-colors relative shrink-0
                          ${value ? "bg-z8-600" : "bg-slate-700"}
                        `}
                      >
                        <div
                          className={`
                            w-3.5 h-3.5 bg-white rounded-full absolute top-[3px] transition-transform
                            ${value ? "translate-x-[19px]" : "translate-x-[3px]"}
                          `}
                        />
                      </button>
                      <span className="text-[10px] text-slate-500">
                        {value ? "Enabled" : "Disabled"}
                      </span>
                    </div>
                  ) : typeof value === "object" && value !== null ? (
                    <textarea
                      value={JSON.stringify(value, null, 2)}
                      onChange={(e) => {
                        try {
                          handleConfigChange(key, JSON.parse(e.target.value));
                        } catch {
                          // Don't update if invalid JSON — user is still typing
                        }
                      }}
                      rows={4}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                        transition-colors resize-y"
                      spellCheck={false}
                    />
                  ) : (
                    <input
                      type="text"
                      value={String(value)}
                      onChange={(e) => handleConfigChange(key, e.target.value)}
                      className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-1.5
                        text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                        transition-colors"
                    />
                  )}
                </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Ports section */}
        {(inputs.length > 0 || outputs.length > 0) && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <Zap size={12} className="text-slate-500" />
              <span className="text-[10px] font-semibold text-slate-500 uppercase tracking-wider">
                Ports
              </span>
            </div>

            {inputs.length > 0 && (
              <div className="mb-2">
                <div className="text-[10px] text-slate-600 mb-1">Inputs</div>
                <div className="space-y-1">
                  {inputs.map((p) => (
                    <div
                      key={p.id}
                      className="flex items-center gap-2 px-2 py-1 bg-slate-800/50 rounded text-xs"
                    >
                      <div
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{ backgroundColor: PORT_COLORS[p.type] ?? "#94A3B8" }}
                      />
                      <span className="text-slate-300">{p.name}</span>
                      <span className="text-slate-600 font-mono text-[10px] ml-auto">{p.type}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {outputs.length > 0 && (
              <div>
                <div className="text-[10px] text-slate-600 mb-1">Outputs</div>
                <div className="space-y-1">
                  {outputs.map((p) => (
                    <div
                      key={p.id}
                      className="flex items-center gap-2 px-2 py-1 bg-slate-800/50 rounded text-xs"
                    >
                      <div
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{ backgroundColor: PORT_COLORS[p.type] ?? "#94A3B8" }}
                      />
                      <span className="text-slate-300">{p.name}</span>
                      <span className="text-slate-600 font-mono text-[10px] ml-auto">{p.type}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-slate-700">
        <div className="text-[10px] text-slate-600 font-mono truncate">
          ID: {selectedNodeId}
        </div>
      </div>
    </div>
  );
}
