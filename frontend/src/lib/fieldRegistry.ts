/**
 * Field metadata registry for ConfigPanel.
 * Maps (nodeType, fieldKey) → rendering instructions,
 * eliminating 1700+ lines of if/else branches.
 */

// ─── Field descriptor types ──────────────────────────────────────

export interface SelectOption {
  value: string | number;
  label: string;
}

export interface FieldDescriptor {
  type:
    | "text"
    | "number"
    | "number-unit"
    | "select"
    | "textarea"
    | "checkbox"
    | "vault"
    | "code-editor";
  /** Placeholder text */
  placeholder?: string;
  /** Default value if not set */
  defaultValue?: unknown;
  /** For select fields */
  options?: SelectOption[];
  /** For number-unit fields (e.g. "ms", "chars", "words") */
  unit?: string;
  /** For number fields */
  min?: number;
  max?: number;
  step?: number;
  /** For textarea / code-editor */
  rows?: number;
  language?: string;
  /** For vault fields: suggested key name template (supports {provider} token) */
  suggestedKeyName?: string;
  /** For checkboxes: description text */
  checkboxLabel?: string;
  /** Hint text shown below the input */
  hint?: string;
  /** Whether to use monospace font */
  mono?: boolean;
  /** Whether the field is resizable (textarea) */
  resizable?: boolean;
}

// ─── Shared option sets ──────────────────────────────────────────

export const HTTP_METHODS: SelectOption[] = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
].map((m) => ({ value: m, label: m }));

export const HTTP_METHODS_WITH_ANY: SelectOption[] = [
  { value: "ANY", label: "ANY" },
  ...HTTP_METHODS,
];

export const DB_TYPES: SelectOption[] = [
  { value: "postgres", label: "PostgreSQL" },
  { value: "mysql", label: "MySQL" },
  { value: "sqlite", label: "SQLite" },
  { value: "mssql", label: "SQL Server" },
];

export const HTTP_STATUS_CODES: SelectOption[] = [
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

// ─── Registry type ───────────────────────────────────────────────

type FieldKey = string; // "fieldKey" or "nodeType:fieldKey"

/**
 * The field registry maps field keys to their descriptors.
 * Keys can be:
 * - "fieldKey"               → matches any node type
 * - "nodeType:fieldKey"      → matches only that node type (higher priority)
 */
export const FIELD_REGISTRY: Record<FieldKey, FieldDescriptor> = {
  // ─── Global fields (any node type) ─────────────────────────────
  method: {
    type: "select",
    options: HTTP_METHODS,
    defaultValue: "GET",
  },
  statusCode: {
    type: "select",
    options: HTTP_STATUS_CODES,
    defaultValue: 200,
  },
  url: {
    type: "text",
    placeholder: "https://api.example.com/endpoint",
    mono: true,
  },
  timeout: {
    type: "number-unit",
    defaultValue: 5000,
    min: 100,
    step: 500,
    unit: "ms",
  },

  // ─── JSON Transform ────────────────────────────────────────────
  "json:action": {
    type: "select",
    defaultValue: "parse",
    options: [
      { value: "parse", label: "Parse (string → object)" },
      { value: "stringify", label: "Stringify (object → string)" },
      { value: "extract", label: "Extract (dot-notation path)" },
    ],
  },

  // ─── CSV ───────────────────────────────────────────────────────
  "csv:action": {
    type: "select",
    defaultValue: "parse",
    options: [
      { value: "parse", label: "Parse (CSV → JSON)" },
      { value: "stringify", label: "Stringify (JSON → CSV)" },
    ],
  },
  "csv:delimiter": {
    type: "select",
    defaultValue: ",",
    options: [
      { value: ",", label: ", (comma)" },
      { value: ";", label: "; (semicolon)" },
      { value: "tab", label: "Tab" },
      { value: "|", label: "| (pipe)" },
    ],
  },

  // ─── Aggregator ────────────────────────────────────────────────
  "aggregator:operation": {
    type: "select",
    defaultValue: "count",
    options: [
      { value: "count", label: "Count" },
      { value: "sum", label: "Sum" },
      { value: "avg", label: "Average" },
      { value: "min", label: "Min" },
      { value: "max", label: "Max" },
      { value: "group_by", label: "Group By" },
    ],
  },
  "aggregator:field": {
    type: "text",
    placeholder: "e.g. amount, price, score",
  },
  "aggregator:groupBy": {
    type: "text",
    placeholder: "e.g. category, status, region",
  },

  // ─── Batch ─────────────────────────────────────────────────────
  "batch:size": {
    type: "number-unit",
    defaultValue: 100,
    min: 1,
    step: 10,
    unit: "items/batch",
  },
  "batch:field": {
    type: "text",
    placeholder: "auto-detect (rows, data, results)",
  },

  // ─── Database ──────────────────────────────────────────────────
  "database:dbType": {
    type: "select",
    defaultValue: "postgres",
    options: DB_TYPES,
  },
  "database:host": {
    type: "text",
    placeholder: "localhost",
    defaultValue: "localhost",
  },
  "database:port": {
    type: "number",
    defaultValue: 5432,
    min: 1,
    max: 65535,
  },
  "database:database": {
    type: "text",
    placeholder: "my_database",
  },
  "database:user": {
    type: "text",
    placeholder: "postgres",
  },
  "database:password": {
    type: "vault",
    placeholder: "Database password",
    suggestedKeyName: "db-{provider}-password",
  },
  "database:connectionString": {
    type: "text",
    placeholder: "postgres://user:pass@host:5432/db",
    mono: true,
    hint: "If set, overrides individual host/port/user/password fields",
  },

  // ─── Delay ─────────────────────────────────────────────────────
  "delay:delay": {
    type: "number-unit",
    defaultValue: 1000,
    min: 0,
    step: 100,
    unit: "ms",
  },
  "delay:unit": {
    type: "select",
    defaultValue: "ms",
    options: [
      { value: "ms", label: "Milliseconds" },
      { value: "s", label: "Seconds" },
      { value: "m", label: "Minutes" },
    ],
  },

  // ─── Timer ─────────────────────────────────────────────────────
  "timer:interval": {
    type: "number-unit",
    defaultValue: 5000,
    min: 100,
    step: 500,
    unit: "ms",
  },

  // ─── Filter ────────────────────────────────────────────────────
  "filter:property": {
    type: "text",
    placeholder: "payload.status",
    mono: true,
  },
  "filter:condition": {
    type: "select",
    defaultValue: "notempty",
    options: [
      { value: "notempty", label: "Not empty" },
      { value: "empty", label: "Empty" },
      { value: "eq", label: "Equals" },
      { value: "neq", label: "Not equals" },
      { value: "gt", label: "Greater than" },
      { value: "lt", label: "Less than" },
      { value: "contains", label: "Contains" },
      { value: "regex", label: "Matches regex" },
    ],
  },
  "filter:passExpression": {
    type: "text",
    placeholder: '{"score": ".age * 2"}',
    mono: true,
    hint: 'Transform for Pass output. Use ".field" refs + math operators',
  },
  "filter:rejectExpression": {
    type: "text",
    placeholder: '{"reason": "rejected"}',
    mono: true,
    hint: 'Transform for Reject output. Use ".field" refs + math operators',
  },

  // ─── Text Splitter ─────────────────────────────────────────────
  "text-splitter:strategy": {
    type: "select",
    defaultValue: "fixed",
    options: [
      { value: "fixed", label: "Fixed size" },
      { value: "sentences", label: "By sentences" },
      { value: "paragraphs", label: "By paragraphs" },
      { value: "tokens", label: "By tokens (approx)" },
    ],
  },
  "text-splitter:chunkSize": {
    type: "number-unit",
    defaultValue: 512,
    min: 50,
    step: 50,
    unit: "chars",
  },
  "text-splitter:overlap": {
    type: "number-unit",
    defaultValue: 50,
    min: 0,
    step: 10,
    unit: "chars",
  },

  // ─── Vector Store ──────────────────────────────────────────────
  "vector-store:action": {
    type: "select",
    defaultValue: "store",
    options: [
      { value: "store", label: "Store embedding" },
      { value: "search", label: "Search similar" },
      { value: "delete", label: "Delete entry" },
      { value: "clear", label: "Clear collection" },
    ],
  },
  "vector-store:collection": {
    type: "text",
    placeholder: "Collection name",
    defaultValue: "default",
  },
  "vector-store:topK": {
    type: "number",
    defaultValue: 5,
    min: 1,
    max: 100,
  },
  "vector-store:minScore": {
    type: "number",
    defaultValue: 0.0,
    min: 0,
    max: 1,
    step: 0.05,
  },

  // ─── Structured Output ─────────────────────────────────────────
  "structured-output:schema": {
    type: "code-editor",
    language: "json",
    placeholder: '{"type":"object","properties":{"name":{"type":"string"}}}',
  },
  "structured-output:retries": {
    type: "number",
    defaultValue: 2,
    min: 0,
    max: 5,
  },

  // ─── Summarizer ────────────────────────────────────────────────
  "summarizer:strategy": {
    type: "select",
    defaultValue: "simple",
    options: [
      { value: "simple", label: "Simple (full text)" },
      { value: "map-reduce", label: "Map-Reduce (long docs)" },
    ],
  },
  "summarizer:maxLength": {
    type: "number-unit",
    defaultValue: 200,
    min: 50,
    step: 50,
    unit: "words",
  },
  "summarizer:language": {
    type: "text",
    placeholder: "Output language (empty = same as input)",
  },

  // ─── Prompt Template ───────────────────────────────────────────
  "prompt-template:template": {
    type: "textarea",
    placeholder: "Hello {{name}}, you asked: {{prompt}}",
    rows: 5,
    mono: true,
    resizable: true,
  },

  // ─── AI Agent ──────────────────────────────────────────────────
  "ai-agent:tools": {
    type: "code-editor",
    language: "json",
    placeholder:
      '[{"name":"search","description":"Search the web","parameters":{...}}]',
  },
  "ai-agent:maxIterations": {
    type: "number",
    defaultValue: 5,
    min: 1,
    max: 20,
  },

  // ─── Image Gen ─────────────────────────────────────────────────
  "image-gen:size": {
    type: "select",
    defaultValue: "1024x1024",
    options: [
      { value: "1024x1024", label: "1024 x 1024 (Square)" },
      { value: "1024x1792", label: "1024 x 1792 (Portrait)" },
      { value: "1792x1024", label: "1792 x 1024 (Landscape)" },
    ],
  },
  "image-gen:quality": {
    type: "select",
    defaultValue: "standard",
    options: [
      { value: "standard", label: "Standard" },
      { value: "hd", label: "HD" },
    ],
  },
  "image-gen:style": {
    type: "select",
    defaultValue: "vivid",
    options: [
      { value: "vivid", label: "Vivid" },
      { value: "natural", label: "Natural" },
    ],
  },

  // ─── MQTT ──────────────────────────────────────────────────────
  "mqtt:action": {
    type: "select",
    defaultValue: "publish",
    options: [
      { value: "publish", label: "Publish" },
      { value: "subscribe", label: "Subscribe" },
    ],
  },
  "mqtt:broker": {
    type: "text",
    placeholder: "localhost",
    defaultValue: "localhost",
  },
  "mqtt:port": {
    type: "number",
    defaultValue: 1883,
    min: 1,
    max: 65535,
  },
  "mqtt:topic": {
    type: "text",
    placeholder: "z8run/default",
    defaultValue: "z8run/default",
  },
  "mqtt:qos": {
    type: "select",
    defaultValue: 0,
    options: [
      { value: 0, label: "0 — At most once" },
      { value: 1, label: "1 — At least once" },
      { value: 2, label: "2 — Exactly once" },
    ],
  },
  "mqtt:clientId": {
    type: "text",
    placeholder: "auto-generated",
  },
  "mqtt:username": {
    type: "text",
    placeholder: "optional",
  },
  "mqtt:password": {
    type: "vault",
    placeholder: "MQTT password (optional)",
    suggestedKeyName: "mqtt-password",
  },
  "mqtt:keepAlive": {
    type: "number-unit",
    defaultValue: 30,
    min: 1,
    step: 1,
    unit: "s",
  },

  // ─── LLM ───────────────────────────────────────────────────────
  "llm:vision": {
    type: "checkbox",
    checkboxLabel: "Enable image analysis (GPT-4o, Claude, LLaVA)",
  },
  "llm:systemPrompt": {
    type: "textarea",
    placeholder: "You are a helpful assistant...",
    rows: 4,
    resizable: true,
  },
  "ai-agent:systemPrompt": {
    type: "textarea",
    placeholder: "You are a helpful assistant...",
    rows: 4,
    resizable: true,
  },

  // ─── Temperature/MaxTokens (shared across AI nodes) ────────────
  temperature: {
    type: "number",
    defaultValue: 0.7,
    min: 0,
    max: 2,
    step: 0.1,
  },
  maxTokens: {
    type: "number",
    defaultValue: 1024,
    min: 1,
  },

  // ─── Classifier ────────────────────────────────────────────────
  "classifier:categories": {
    type: "text",
    placeholder: "positive, negative, neutral",
  },
  "classifier:context": {
    type: "text",
    placeholder: "What are you classifying?",
  },

  // ─── Twilio ────────────────────────────────────────────────────
  "twilio:action": {
    type: "select",
    defaultValue: "sms",
    options: [
      { value: "sms", label: "Send SMS" },
      { value: "call", label: "Make Call" },
      { value: "lookup", label: "Lookup Number" },
    ],
  },
  "twilio:accountSid": {
    type: "text",
    placeholder: "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  },
  "twilio:authToken": {
    type: "vault",
    placeholder: "Twilio Auth Token",
    suggestedKeyName: "twilio-auth-token",
  },
  "twilio:fromNumber": {
    type: "text",
    placeholder: "+1234567890",
  },

  // ─── WhatsApp ──────────────────────────────────────────────────
  "whatsapp:action": {
    type: "select",
    defaultValue: "send_text",
    options: [
      { value: "send_text", label: "Send Text" },
      { value: "send_template", label: "Send Template" },
      { value: "send_media", label: "Send Media" },
    ],
  },
  "whatsapp:phoneNumberId": {
    type: "text",
    placeholder: "Phone Number ID",
  },
  "whatsapp:accessToken": {
    type: "vault",
    placeholder: "Meta Access Token",
    suggestedKeyName: "whatsapp-access-token",
  },
  "whatsapp:apiVersion": {
    type: "select",
    defaultValue: "v18.0",
    options: [
      { value: "v18.0", label: "v18.0" },
      { value: "v19.0", label: "v19.0" },
      { value: "v20.0", label: "v20.0" },
    ],
  },

  // ─── Conversation Memory ───────────────────────────────────────
  "conversation-memory:action": {
    type: "select",
    defaultValue: "save",
    options: [
      { value: "save", label: "Save Message" },
      { value: "load", label: "Load History" },
      { value: "clear", label: "Clear Conversation" },
      { value: "list", label: "List Conversations" },
    ],
  },
  "conversation-memory:maxMessages": {
    type: "number-unit",
    defaultValue: 50,
    min: 1,
    max: 500,
    unit: "msgs",
  },
  "conversation-memory:ttlSeconds": {
    type: "number-unit",
    defaultValue: 3600,
    min: 60,
    step: 60,
    unit: "sec",
  },

  // ─── CRM ───────────────────────────────────────────────────────
  "crm:provider": {
    type: "select",
    defaultValue: "hubspot",
    options: [
      { value: "hubspot", label: "HubSpot" },
      { value: "salesforce", label: "Salesforce" },
    ],
  },
  "crm:action": {
    type: "select",
    defaultValue: "create_contact",
    options: [
      { value: "create_contact", label: "Create Contact" },
      { value: "update_contact", label: "Update Contact" },
      { value: "get_contact", label: "Get Contact" },
      { value: "search_contacts", label: "Search Contacts" },
      { value: "create_deal", label: "Create Deal" },
      { value: "list_deals", label: "List Deals" },
    ],
  },
  "crm:apiKey": {
    type: "vault",
    placeholder: "API Key / Access Token",
    suggestedKeyName: "{provider}-api-key",
  },
  "crm:baseUrl": {
    type: "text",
    placeholder: "Salesforce instance URL (optional)",
  },

  // ─── Human Handoff ─────────────────────────────────────────────
  "human-handoff:action": {
    type: "select",
    defaultValue: "escalate",
    options: [
      { value: "escalate", label: "Escalate" },
      { value: "check_status", label: "Check Status" },
      { value: "assign", label: "Assign Agent" },
      { value: "resolve", label: "Resolve" },
    ],
  },
  "human-handoff:priority": {
    type: "select",
    defaultValue: "medium",
    options: [
      { value: "low", label: "Low" },
      { value: "medium", label: "Medium" },
      { value: "high", label: "High" },
      { value: "urgent", label: "Urgent" },
    ],
  },
  "human-handoff:webhookUrl": {
    type: "text",
    placeholder: "https://your-system.com/webhook (optional)",
  },

  // ─── If/Else ───────────────────────────────────────────────────
  "if-else:field": {
    type: "text",
    placeholder: "payload.status",
    mono: true,
  },
  "if-else:operator": {
    type: "select",
    defaultValue: "==",
    options: [
      { value: "==", label: "== (equals)" },
      { value: "!=", label: "!= (not equal)" },
      { value: ">", label: "> (greater than)" },
      { value: "<", label: "< (less than)" },
      { value: ">=", label: ">= (greater or equal)" },
      { value: "<=", label: "<= (less or equal)" },
      { value: "contains", label: "contains" },
      { value: "not_contains", label: "not contains" },
      { value: "starts_with", label: "starts with" },
      { value: "ends_with", label: "ends with" },
      { value: "matches", label: "matches (regex)" },
      { value: "exists", label: "exists" },
      { value: "not_exists", label: "not exists" },
      { value: "is_empty", label: "is empty" },
      { value: "is_not_empty", label: "is not empty" },
    ],
  },
  "if-else:trueExpression": {
    type: "textarea",
    placeholder: '{ "amount": ".amount * 10" }',
    rows: 3,
    mono: true,
    resizable: true,
    hint: 'JSON mapping for True branch. Use ".field" to reference input values, supports * + - /',
  },
  "if-else:falseExpression": {
    type: "textarea",
    placeholder: '{ "amount": ".amount * 5" }',
    rows: 3,
    mono: true,
    resizable: true,
    hint: 'JSON mapping for False branch. Use ".field" to reference input values, supports * + - /',
  },

  // ─── Loop ──────────────────────────────────────────────────────
  "loop:field": {
    type: "text",
    placeholder: "payload.items",
    mono: true,
  },
  "loop:itemExpression": {
    type: "text",
    placeholder: '{"name": ".name", "total": ".price * .qty"}',
    mono: true,
    hint: 'Transform each item. ".field" references item properties',
  },

  // ─── Cron Trigger ──────────────────────────────────────────────
  "cron-trigger:cron": {
    type: "text",
    placeholder: "0 * * * *",
    mono: true,
    hint: 'Format: min hour dom month dow (e.g. "0 9 * * 1-5" = weekdays 9am)',
  },
  "cron-trigger:timezone": {
    type: "select",
    defaultValue: "UTC",
    options: [
      { value: "UTC", label: "UTC" },
      { value: "America/New_York", label: "America/New_York" },
      { value: "America/Chicago", label: "America/Chicago" },
      { value: "America/Denver", label: "America/Denver" },
      { value: "America/Los_Angeles", label: "America/Los_Angeles" },
      { value: "America/Bogota", label: "America/Bogota" },
      { value: "America/Sao_Paulo", label: "America/Sao_Paulo" },
      { value: "America/Mexico_City", label: "America/Mexico_City" },
      { value: "Europe/London", label: "Europe/London" },
      { value: "Europe/Berlin", label: "Europe/Berlin" },
      { value: "Europe/Madrid", label: "Europe/Madrid" },
      { value: "Asia/Tokyo", label: "Asia/Tokyo" },
      { value: "Asia/Shanghai", label: "Asia/Shanghai" },
    ],
  },

  // ─── Webhook Trigger ───────────────────────────────────────────
  "webhook-trigger:method": {
    type: "select",
    defaultValue: "POST",
    options: HTTP_METHODS_WITH_ANY,
  },
  "webhook-trigger:path": {
    type: "text",
    placeholder: "/my-webhook",
    mono: true,
    hint: "Sub-path after /hook/{flow_id}/",
  },
  "webhook-trigger:authType": {
    type: "select",
    defaultValue: "none",
    options: [
      { value: "none", label: "None" },
      { value: "bearer", label: "Bearer Token" },
      { value: "basic", label: "Basic Auth" },
      { value: "hmac", label: "HMAC Signature" },
    ],
  },
  "webhook-trigger:responseMode": {
    type: "select",
    defaultValue: "last_node",
    options: [
      { value: "last_node", label: "Last Node Output" },
      { value: "immediate", label: "Immediate (202 Accepted)" },
    ],
  },

  // ─── Mapper ────────────────────────────────────────────────────
  "mapper:passThrough": {
    type: "checkbox",
    checkboxLabel: "Include all original fields (merge mode)",
  },

  // ─── Sanitize ──────────────────────────────────────────────────
  "sanitize:fields": {
    type: "textarea",
    placeholder: "headers.authorization, body.password, body.ssn",
    rows: 3,
    mono: true,
    hint: "Comma-separated dot-notation paths to sanitize",
  },
  "sanitize:strategy": {
    type: "select",
    defaultValue: "mask",
    options: [
      { value: "mask", label: "Mask (sk-***x8Y)" },
      { value: "redact", label: "Redact ([REDACTED])" },
      { value: "hash", label: "Hash (SHA-256)" },
      { value: "remove", label: "Remove (set null)" },
    ],
  },
  "sanitize:detectPatterns": {
    type: "checkbox",
    checkboxLabel: "Auto-detect sensitive patterns in all values",
  },

  // ─── HTTP Request ──────────────────────────────────────────────
  "http-request:bodyPath": {
    type: "text",
    placeholder: "req.body",
    defaultValue: "req.body",
    mono: true,
  },

  // ─── Webhook ───────────────────────────────────────────────────
  "webhook:path": {
    type: "text",
    placeholder: "/hook",
    defaultValue: "/hook",
    mono: true,
  },
  "webhook:method": {
    type: "select",
    defaultValue: "POST",
    options: HTTP_METHODS,
  },
  "webhook:secret": {
    type: "vault",
    placeholder: "Webhook secret (optional)",
    suggestedKeyName: "webhook-secret",
  },
  "webhook:signatureHeader": {
    type: "text",
    placeholder: "X-Hub-Signature-256",
    defaultValue: "X-Hub-Signature-256",
  },
  "webhook:eventHeader": {
    type: "text",
    placeholder: "X-GitHub-Event (optional)",
  },
};

/**
 * Look up a field descriptor from the registry.
 * Priority: "nodeType:fieldKey" > "fieldKey"
 */
export function lookupField(
  nodeType: string,
  fieldKey: string,
): FieldDescriptor | undefined {
  return FIELD_REGISTRY[`${nodeType}:${fieldKey}`] ?? FIELD_REGISTRY[fieldKey];
}
