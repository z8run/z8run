import { useState, useCallback, useMemo } from "react";
import {
  Play,
  ChevronDown,
  ChevronUp,
  Copy,
  Check,
  AlertTriangle,
  Sparkles,
} from "lucide-react";
import { NODE_DEFINITIONS } from "@/lib/nodeDefinitions";

/** Mock evaluators that simulate node behavior client-side */
const MOCK_EVALUATORS: Record<
  string,
  (input: Record<string, unknown>, config: Record<string, unknown>) => MockResult
> = {
  "if-else": (input, config) => {
    const field = String(config.field ?? "");
    const operator = String(config.operator ?? "==");
    const compareValue = config.value;
    const fieldValue = extractField(input, field);

    const result = evaluateCondition(fieldValue, operator, compareValue);
    const exprRaw = result
      ? String(config.trueExpression ?? "")
      : String(config.falseExpression ?? "");

    let transformed: unknown = input;
    if (exprRaw.trim()) {
      try {
        const mapping = JSON.parse(exprRaw) as Record<string, string>;
        transformed = applyTransformMapping(input, mapping);
      } catch {
        transformed = {
          ...input,
          _error: `Invalid JSON in ${result ? "trueExpression" : "falseExpression"}`,
        };
      }
    }

    return {
      port: result ? "true" : "false",
      output: {
        ...(typeof transformed === "object" && transformed !== null ? transformed : { value: transformed }),
        _evaluation: { field, operator, value: compareValue, fieldValue, result },
      },
    };
  },
  loop: (input, config) => {
    const field = String(config.field ?? "");
    const arr = extractField(input, field);
    const items = Array.isArray(arr) ? arr : arr != null ? [arr] : [];
    if (items.length === 0) return { port: "done", output: { total: 0 } };

    const firstItem = items[0] as Record<string, unknown>;
    const itemExpr = String(config.itemExpression ?? "").trim();
    let transformedItem: unknown = firstItem;
    if (itemExpr) {
      try {
        const mapping = JSON.parse(itemExpr) as Record<string, string>;
        transformedItem = typeof firstItem === "object" && firstItem !== null
          ? applyTransformMapping(firstItem, mapping)
          : firstItem;
      } catch {
        transformedItem = { ...firstItem, _error: "Invalid itemExpression JSON" };
      }
    }
    return {
      port: "item",
      output: { item: transformedItem, index: 0, total: items.length, isFirst: true, isLast: items.length === 1 },
    };
  },
  filter: (input, config) => {
    const prop = String(config.property ?? "");
    const condition = String(config.condition ?? "eq");
    const val = config.value;
    const fieldValue = extractField(input, prop);
    const pass = evaluateCondition(fieldValue, condition === "gte" ? ">=" : condition === "lte" ? "<=" : "==", val);

    const exprRaw = pass
      ? String(config.passExpression ?? "").trim()
      : String(config.rejectExpression ?? "").trim();

    let transformed: unknown = input;
    if (exprRaw) {
      try {
        const mapping = JSON.parse(exprRaw) as Record<string, string>;
        transformed = applyTransformMapping(input, mapping);
      } catch {
        transformed = { ...input, _error: `Invalid ${pass ? "passExpression" : "rejectExpression"} JSON` };
      }
    }
    return { port: pass ? "pass" : "reject", output: transformed };
  },
  switch: (input, config) => {
    const prop = String(config.property ?? "");
    const rules = (config.rules ?? []) as Array<{ type: string; value: unknown; port: string; transform?: string }>;
    const fieldValue = extractField(input, prop);
    for (const rule of rules) {
      if (evaluateCondition(fieldValue, rule.type === "eq" ? "==" : rule.type, rule.value)) {
        const transformRaw = String(rule.transform ?? "").trim();
        if (transformRaw) {
          try {
            const mapping = JSON.parse(transformRaw) as Record<string, string>;
            return { port: rule.port, output: applyTransformMapping(input, mapping) };
          } catch {
            return { port: rule.port, output: { ...input, _error: "Invalid transform JSON in rule" } };
          }
        }
        return { port: rule.port, output: input };
      }
    }
    return { port: "default", output: input };
  },
  function: (input) => {
    return { port: "output", output: { ...input, _note: "Function nodes run server-side" } };
  },
  json: (input, config) => {
    const action = String(config.action ?? "parse");
    if (action === "parse" && typeof input.payload === "string") {
      try {
        return { port: "output", output: { ...input, payload: JSON.parse(input.payload as string) } };
      } catch {
        return { port: "output", output: input, error: "Invalid JSON string" };
      }
    }
    if (action === "stringify") {
      return { port: "output", output: { ...input, payload: JSON.stringify(input.payload) } };
    }
    return { port: "output", output: input };
  },
  "http-request": (_input, config) => {
    return {
      port: "response",
      output: {
        status: 200,
        headers: { "content-type": "application/json" },
        body: { mock: true, url: config.url, method: config.method },
        _note: "Mock response — real HTTP call requires execution",
      },
    };
  },
  "cron-trigger": (_input, config) => {
    return {
      port: "output",
      output: {
        trigger: "cron",
        cron: config.cron,
        timezone: config.timezone,
        timestamp: new Date().toISOString(),
        payload: config.payload ?? {},
      },
    };
  },
  "webhook-trigger": (_input, config) => {
    return {
      port: "output",
      output: {
        trigger: "webhook",
        method: config.method,
        headers: { "content-type": "application/json", authorization: "Bearer ***" },
        query: {},
        body: { sample: "webhook payload" },
      },
    };
  },
  debug: (input) => {
    return { port: null, output: input };
  },
  delay: (input, config) => {
    return {
      port: "output",
      output: { ...input, _delayed: `${config.delay}${config.unit ?? "ms"}` },
    };
  },
  llm: (_input, config) => {
    return {
      port: "response",
      output: {
        response: `[Mock LLM response for ${config.provider}/${config.model}]`,
        usage: { prompt_tokens: 42, completion_tokens: 18, total_tokens: 60 },
        model: config.model,
        _note: "Mock — real call requires API key and execution",
      },
    };
  },
};

interface MockResult {
  port: string | null;
  output: unknown;
  error?: string;
}

/** Extract a nested field using dot notation: "payload.data.status" */
function extractField(obj: unknown, path: string): unknown {
  if (!path) return obj;
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current == null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

/** Evaluate a simple condition (mirrors the backend if-else logic) */
function evaluateCondition(fieldValue: unknown, operator: string, compareValue: unknown): boolean {
  const fStr = String(fieldValue ?? "");
  const cStr = String(compareValue ?? "");
  const fNum = Number(fieldValue);
  const cNum = Number(compareValue);
  const bothNumeric = !Number.isNaN(fNum) && !Number.isNaN(cNum);

  switch (operator) {
    case "==": case "eq": return fStr === cStr;
    case "!=": case "neq": return fStr !== cStr;
    case ">": case "gt": return bothNumeric ? fNum > cNum : fStr > cStr;
    case "<": case "lt": return bothNumeric ? fNum < cNum : fStr < cStr;
    case ">=": case "gte": return bothNumeric ? fNum >= cNum : fStr >= cStr;
    case "<=": case "lte": return bothNumeric ? fNum <= cNum : fStr <= cStr;
    case "contains": return fStr.includes(cStr);
    case "not_contains": return !fStr.includes(cStr);
    case "starts_with": return fStr.startsWith(cStr);
    case "ends_with": return fStr.endsWith(cStr);
    case "exists": return fieldValue !== undefined && fieldValue !== null;
    case "not_exists": return fieldValue === undefined || fieldValue === null;
    case "is_empty": return fStr === "" || (Array.isArray(fieldValue) && fieldValue.length === 0);
    case "is_not_empty": return fStr !== "" && !(Array.isArray(fieldValue) && fieldValue.length === 0);
    default: return fStr === cStr;
  }
}

/**
 * Evaluate a simple math expression like ".amount * 10".
 * Supports: .field references, numbers, and operators + - * /
 */
function evaluateExpression(expr: string, input: Record<string, unknown>): unknown {
  const trimmed = expr.trim();

  // If it's a static value (no dot references), return as-is
  if (!trimmed.startsWith(".") && !/\.\w/.test(trimmed)) {
    const num = Number(trimmed);
    return Number.isNaN(num) ? trimmed : num;
  }

  // Match pattern: .field <op> <number|.field>
  const mathMatch = trimmed.match(
    /^(\.[\w.]+)\s*([+\-*/])\s*(.+)$/
  );
  if (mathMatch) {
    const leftVal = Number(extractField(input, mathMatch[1]!.slice(1)));
    const rightRaw = mathMatch[3]!.trim();
    const rightVal = rightRaw.startsWith(".")
      ? Number(extractField(input, rightRaw.slice(1)))
      : Number(rightRaw);

    if (!Number.isNaN(leftVal) && !Number.isNaN(rightVal)) {
      switch (mathMatch[2]) {
        case "+": return leftVal + rightVal;
        case "-": return leftVal - rightVal;
        case "*": return leftVal * rightVal;
        case "/": return rightVal !== 0 ? leftVal / rightVal : 0;
      }
    }
  }

  // Simple field reference: ".amount" → input.amount
  if (trimmed.startsWith(".")) {
    return extractField(input, trimmed.slice(1));
  }

  return trimmed;
}

/**
 * Apply a transform mapping object to input data.
 * e.g. { "amount": ".amount * 10", "type": ".type" }
 */
function applyTransformMapping(
  input: Record<string, unknown>,
  mapping: Record<string, string>,
): Record<string, unknown> {
  const result: Record<string, unknown> = { ...input };
  for (const [key, expr] of Object.entries(mapping)) {
    result[key] = evaluateExpression(String(expr), input);
  }
  return result;
}

/** Generate sample input data based on node type and config */
function generateSampleInput(nodeType: string, config: Record<string, unknown>): Record<string, unknown> {
  const def = NODE_DEFINITIONS.find((d) => d.type === nodeType);
  const isInput = def?.category === "input";

  if (isInput) {
    return { _trigger: true };
  }

  switch (nodeType) {
    case "if-else": {
      const field = String(config.field ?? "payload.status");
      const value = config.value ?? "success";
      const parts = field.split(".");
      let obj: Record<string, unknown> = {};
      const root = obj;
      for (let i = 0; i < parts.length - 1; i++) {
        const child: Record<string, unknown> = {};
        obj[parts[i] as string] = child;
        obj = child;
      }
      obj[parts[parts.length - 1] as string] = value;
      return root;
    }
    case "loop":
      return { payload: { items: ["apple", "banana", "cherry"] } };
    case "filter":
      return { req: { body: { age: 25, name: "Alice" } } };
    case "switch":
      return { req: { body: { action: "create" } } };
    case "http-request":
      return { req: { body: { message: "hello" } } };
    case "json":
      return config.action === "stringify"
        ? { payload: { key: "value", count: 42 } }
        : { payload: '{"key":"value","count":42}' };
    case "llm":
    case "ai-agent":
    case "classifier":
    case "summarizer":
    case "structured-output":
      return { prompt: "Sample prompt text", context: "Additional context" };
    case "embeddings":
      return { text: "Sample text for embeddings" };
    case "csv":
      return { payload: "name,age\nAlice,30\nBob,25" };
    case "aggregator":
      return { payload: [{ value: 10 }, { value: 20 }, { value: 30 }] };
    case "batch":
      return { payload: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] };
    default:
      return { payload: { data: "sample", timestamp: new Date().toISOString() } };
  }
}

interface NodeTestPanelProps {
  nodeType: string;
  config: Record<string, unknown>;
}

export function NodeTestPanel({ nodeType, config }: NodeTestPanelProps) {
  const sampleInput = useMemo(() => generateSampleInput(nodeType, config), [nodeType, config]);
  const [inputText, setInputText] = useState(() => JSON.stringify(sampleInput, null, 2));
  const [result, setResult] = useState<MockResult | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);

  const runTest = useCallback(() => {
    try {
      const parsed = JSON.parse(inputText);
      setParseError(null);

      const evaluator = MOCK_EVALUATORS[nodeType];
      if (evaluator) {
        const res = evaluator(parsed, config);
        setResult(res);
      } else {
        // Generic pass-through for nodes without a specific evaluator
        const def = NODE_DEFINITIONS.find((d) => d.type === nodeType);
        const firstOutput = def?.outputs[0]?.id ?? "output";
        setResult({
          port: firstOutput,
          output: {
            ...parsed,
            _mock: true,
            _nodeType: nodeType,
            _note: `No specific mock for "${nodeType}" — showing pass-through`,
          },
        });
      }
    } catch (e) {
      setParseError(e instanceof Error ? e.message : "Invalid JSON");
      setResult(null);
    }
  }, [inputText, nodeType, config]);

  const copyOutput = useCallback(() => {
    if (result) {
      navigator.clipboard.writeText(JSON.stringify(result.output, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }, [result]);

  const hasEvaluator = nodeType in MOCK_EVALUATORS;

  return (
    <div className="border border-slate-700 rounded-lg overflow-hidden">
      {/* Toggle header */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 bg-slate-800/50 hover:bg-slate-800 transition-colors text-left"
      >
        <Play size={12} className="text-emerald-400" />
        <span className="text-[10px] font-semibold text-slate-400 uppercase tracking-wider flex-1">
          Test / Mock
        </span>
        {hasEvaluator && (
          <Sparkles size={10} className="text-amber-400" />
        )}
        {expanded ? (
          <ChevronUp size={14} className="text-slate-500" />
        ) : (
          <ChevronDown size={14} className="text-slate-500" />
        )}
      </button>

      {expanded && (
        <div className="p-3 space-y-3 border-t border-slate-700/50">
          {/* Input editor */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <span className="text-[10px] text-slate-500 font-medium">Input Data (JSON)</span>
              <button
                type="button"
                onClick={() => setInputText(JSON.stringify(generateSampleInput(nodeType, config), null, 2))}
                className="text-[10px] text-z8-400 hover:text-z8-300 transition-colors"
              >
                Reset sample
              </button>
            </div>
            <textarea
              value={inputText}
              onChange={(e) => {
                setInputText(e.target.value);
                setParseError(null);
              }}
              rows={6}
              spellCheck={false}
              className="w-full bg-slate-950 border border-slate-700 rounded-md px-3 py-2
                text-xs text-slate-200 font-mono focus:outline-none focus:border-z8-500
                transition-colors resize-y leading-relaxed"
              placeholder='{ "payload": { ... } }'
            />
            {parseError && (
              <div className="flex items-center gap-1.5 mt-1 text-red-400">
                <AlertTriangle size={10} />
                <span className="text-[10px]">{parseError}</span>
              </div>
            )}
          </div>

          {/* Run button */}
          <button
            type="button"
            onClick={runTest}
            className="w-full flex items-center justify-center gap-2 px-3 py-2
              bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium
              rounded-md transition-colors"
          >
            <Play size={12} />
            Run Test
          </button>

          {/* Output */}
          {result && (
            <div>
              <div className="flex items-center justify-between mb-1">
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-slate-500 font-medium">Output</span>
                  {result.port && (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-900/50 text-emerald-400 font-mono">
                      → {result.port}
                    </span>
                  )}
                </div>
                <button
                  type="button"
                  onClick={copyOutput}
                  className="flex items-center gap-1 text-[10px] text-slate-500 hover:text-slate-300 transition-colors"
                >
                  {copied ? <Check size={10} className="text-emerald-400" /> : <Copy size={10} />}
                  {copied ? "Copied" : "Copy"}
                </button>
              </div>

              {result.error && (
                <div className="flex items-center gap-1.5 mb-2 px-2 py-1.5 bg-red-900/20 border border-red-800/30 rounded text-red-400">
                  <AlertTriangle size={10} />
                  <span className="text-[10px]">{result.error}</span>
                </div>
              )}

              <pre className="bg-slate-950 border border-slate-700 rounded-md px-3 py-2
                text-xs text-slate-300 font-mono overflow-x-auto max-h-[200px] overflow-y-auto leading-relaxed">
                {JSON.stringify(result.output, null, 2)}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
