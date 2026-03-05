import type { NodeCategory, PortDefinition, Z8NodeData } from "@/types/flow";

export interface NodeDefinition {
  type: string;
  label: string;
  category: NodeCategory;
  icon: string;
  description: string;
  inputs: PortDefinition[];
  outputs: PortDefinition[];
  defaultConfig: Record<string, unknown>;
}

export const NODE_DEFINITIONS: NodeDefinition[] = [
  // Input nodes
  {
    type: "http-in",
    label: "HTTP In",
    category: "input",
    icon: "Globe",
    description: "Receive HTTP requests",
    inputs: [],
    outputs: [{ id: "response", name: "Response", type: "object" }],
    defaultConfig: { method: "POST", path: "/" },
  },
  {
    type: "timer",
    label: "Timer",
    category: "input",
    icon: "Clock",
    description: "Trigger on interval",
    inputs: [],
    outputs: [{ id: "tick", name: "Tick", type: "object" }],
    defaultConfig: { interval: 5000, unit: "ms" },
  },
  {
    type: "webhook",
    label: "Webhook",
    category: "input",
    icon: "Webhook",
    description: "Listen for webhook events",
    inputs: [],
    outputs: [{ id: "payload", name: "Payload", type: "object" }],
    defaultConfig: { path: "/hook", method: "POST" },
  },

  // Process nodes
  {
    type: "function",
    label: "Function",
    category: "process",
    icon: "Code",
    description: "Run custom JavaScript",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [{ id: "output", name: "Output", type: "any" }],
    defaultConfig: { code: "return msg;" },
  },
  {
    type: "json",
    label: "JSON Transform",
    category: "process",
    icon: "Braces",
    description: "Parse or stringify JSON",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [{ id: "output", name: "Output", type: "object" }],
    defaultConfig: { action: "parse" },
  },
  {
    type: "http-request",
    label: "HTTP Request",
    category: "process",
    icon: "Send",
    description: "Make HTTP requests to external APIs",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "response", name: "Response", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: { url: "https://httpbin.org/post", method: "POST", headers: {}, bodyPath: "req.body", timeout: 5000 },
  },
  {
    type: "filter",
    label: "Filter",
    category: "process",
    icon: "Filter",
    description: "Filter messages by condition",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "pass", name: "Pass", type: "any" },
      { id: "reject", name: "Reject", type: "any" },
    ],
    defaultConfig: { property: "req.body.age", condition: "gte", value: 18 },
  },

  // Output nodes
  {
    type: "debug",
    label: "Debug",
    category: "output",
    icon: "Bug",
    description: "Log messages to debug panel",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [],
    defaultConfig: { output: "full", console: false },
  },
  {
    type: "http-out",
    label: "HTTP Response",
    category: "output",
    icon: "Send",
    description: "Send HTTP response",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [],
    defaultConfig: { statusCode: 200 },
  },

  // Logic nodes
  {
    type: "switch",
    label: "Switch",
    category: "logic",
    icon: "GitBranch",
    description: "Route by condition",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "out1", name: "Case 1", type: "any" },
      { id: "out2", name: "Case 2", type: "any" },
      { id: "default", name: "Default", type: "any" },
    ],
    defaultConfig: { property: "req.body.action", rules: [
      { type: "eq", value: "create", port: "out1" },
      { type: "eq", value: "update", port: "out2" },
    ] },
  },
  {
    type: "delay",
    label: "Delay",
    category: "logic",
    icon: "Timer",
    description: "Delay message delivery",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [{ id: "output", name: "Output", type: "any" }],
    defaultConfig: { delay: 1000, unit: "ms" },
  },

  // Data nodes
  {
    type: "database",
    label: "Database",
    category: "data",
    icon: "Database",
    description: "Query a database",
    inputs: [{ id: "query", name: "Query", type: "string" }],
    outputs: [
      { id: "results", name: "Results", type: "array" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      dbType: "postgres",
      host: "localhost",
      port: 5432,
      database: "",
      user: "",
      password: "",
      query: "",
      params: [],
    },
  },
  {
    type: "mqtt",
    label: "MQTT",
    category: "data",
    icon: "Radio",
    description: "Publish/subscribe to MQTT broker",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "message", name: "Message", type: "object" },
      { id: "published", name: "Published", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      action: "publish",
      broker: "localhost",
      port: 1883,
      topic: "z8run/default",
      qos: 0,
      clientId: "",
      username: "",
      password: "",
      useTls: false,
      keepAlive: 30,
      timeout: 30000,
    },
  },

  // AI nodes
  {
    type: "llm",
    label: "LLM",
    category: "ai",
    icon: "Brain",
    description: "AI language model (OpenAI, Anthropic, Ollama)",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "response", name: "Response", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "gpt-4o-mini",
      apiKey: "",
      baseUrl: "",
      systemPrompt: "",
      temperature: 0.7,
      maxTokens: 1024,
      timeout: 30000,
    },
  },
  {
    type: "embeddings",
    label: "Embeddings",
    category: "ai",
    icon: "Fingerprint",
    description: "Generate vector embeddings from text",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "embedding", name: "Embedding", type: "array" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "text-embedding-3-small",
      apiKey: "",
      baseUrl: "",
      timeout: 15000,
    },
  },
  {
    type: "classifier",
    label: "Classifier",
    category: "ai",
    icon: "Tags",
    description: "Classify text into categories using AI",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "result", name: "Result", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "gpt-4o-mini",
      apiKey: "",
      baseUrl: "",
      categories: "positive,negative,neutral",
      context: "",
      timeout: 15000,
    },
  },
  {
    type: "prompt-template",
    label: "Prompt Template",
    category: "ai",
    icon: "FileText",
    description: "Render templates with {{variable}} injection",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "output", name: "Output", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      template: "Hello {{name}}, you asked: {{prompt}}",
      strictMode: false,
    },
  },
  {
    type: "text-splitter",
    label: "Text Splitter",
    category: "ai",
    icon: "Scissors",
    description: "Split text into chunks for RAG pipelines",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "chunks", name: "Chunks", type: "array" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      strategy: "fixed",
      chunkSize: 512,
      overlap: 50,
      separator: "",
    },
  },
  {
    type: "vector-store",
    label: "Vector Store",
    category: "ai",
    icon: "Database",
    description: "Store and search vector embeddings",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "stored", name: "Stored", type: "object" },
      { id: "results", name: "Results", type: "array" },
      { id: "deleted", name: "Deleted", type: "object" },
      { id: "cleared", name: "Cleared", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      action: "store",
      collection: "default",
      topK: 5,
      minScore: 0.0,
    },
  },
  {
    type: "structured-output",
    label: "Structured Output",
    category: "ai",
    icon: "Braces",
    description: "Force LLM output into JSON schema",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "output", name: "Output", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "gpt-4o-mini",
      apiKey: "",
      baseUrl: "",
      schema: '{"type":"object","properties":{"name":{"type":"string"},"score":{"type":"number"}}}',
      retries: 2,
      timeout: 30000,
    },
  },
  {
    type: "summarizer",
    label: "Summarizer",
    category: "ai",
    icon: "AlignLeft",
    description: "Summarize text with map-reduce for long docs",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "summary", name: "Summary", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "gpt-4o-mini",
      apiKey: "",
      baseUrl: "",
      strategy: "simple",
      maxLength: 200,
      language: "",
      timeout: 60000,
    },
  },
  {
    type: "ai-agent",
    label: "AI Agent",
    category: "ai",
    icon: "Bot",
    description: "LLM agent with tool-use capabilities",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "response", name: "Response", type: "object" },
      { id: "tool_call", name: "Tool Call", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "gpt-4o",
      apiKey: "",
      baseUrl: "",
      systemPrompt: "You are a helpful assistant.",
      tools: "[]",
      maxIterations: 5,
      temperature: 0.7,
      timeout: 60000,
    },
  },
  {
    type: "image-gen",
    label: "Image Gen",
    category: "ai",
    icon: "Image",
    description: "Generate images with DALL-E or Stability AI",
    inputs: [{ id: "input", name: "Input", type: "any" }],
    outputs: [
      { id: "image", name: "Image", type: "object" },
      { id: "error", name: "Error", type: "any" },
    ],
    defaultConfig: {
      provider: "openai",
      model: "dall-e-3",
      apiKey: "",
      baseUrl: "",
      size: "1024x1024",
      quality: "standard",
      style: "vivid",
      timeout: 60000,
    },
  },
];

export const NODE_CATEGORIES: { id: NodeCategory; label: string }[] = [
  { id: "input", label: "Input" },
  { id: "process", label: "Process" },
  { id: "output", label: "Output" },
  { id: "logic", label: "Logic" },
  { id: "data", label: "Data" },
  { id: "ai", label: "AI" },
];

/** Create Z8NodeData from a NodeDefinition */
export function createNodeData(def: NodeDefinition): Z8NodeData {
  return {
    label: def.label,
    type: def.type,
    category: def.category,
    icon: def.icon,
    config: { ...def.defaultConfig },
    status: "idle",
    inputs: def.inputs,
    outputs: def.outputs,
  };
}
