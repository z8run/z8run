import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { sql } from "@codemirror/lang-sql";
import { EditorView } from "@codemirror/view";
import CodeMirror from "@uiw/react-codemirror";
import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";

/** Supported languages for the code editor */
export const CODE_LANGUAGES = [
  { value: "javascript", label: "JavaScript" },
  { value: "typescript", label: "TypeScript" },
  { value: "python", label: "Python" },
  { value: "sql", label: "SQL" },
  { value: "json", label: "JSON" },
  { value: "rust", label: "Rust" },
  { value: "shell", label: "Shell" },
  { value: "text", label: "Plain Text" },
] as const;

export type CodeLanguage = (typeof CODE_LANGUAGES)[number]["value"];

/** Auto-detect language from code content */
export function detectLanguage(code: string): CodeLanguage {
  const trimmed = code.trim();

  // JSON detection
  if (
    (trimmed.startsWith("{") && trimmed.endsWith("}")) ||
    (trimmed.startsWith("[") && trimmed.endsWith("]"))
  ) {
    try {
      JSON.parse(trimmed);
      return "json";
    } catch {
      // not valid JSON
    }
  }

  // SQL detection
  if (
    /^(SELECT|INSERT|UPDATE|DELETE|CREATE|ALTER|DROP|WITH)\s/i.test(trimmed)
  ) {
    return "sql";
  }

  // Python detection
  if (
    /^(def |class |import |from |print\(|if __name__|#!.*python)/m.test(trimmed)
  ) {
    return "python";
  }

  // Shell detection
  if (/^(#!\/bin\/(ba)?sh|curl |wget |echo |export |source )/m.test(trimmed)) {
    return "shell";
  }

  // Rust detection
  if (/^(fn |let |use |mod |pub |struct |impl |enum )/m.test(trimmed)) {
    return "rust";
  }

  // TypeScript detection (before JS — checks for type annotations)
  if (
    /:\s*(string|number|boolean|any|void|never|Record|Promise)\b/.test(
      trimmed,
    ) ||
    /interface\s+\w+/.test(trimmed) ||
    /type\s+\w+\s*=/.test(trimmed)
  ) {
    return "typescript";
  }

  // Default: JavaScript (most common in z8run flows)
  return "javascript";
}

/** Get CodeMirror language extension for a given language */
function getLanguageExtension(lang: CodeLanguage) {
  switch (lang) {
    case "javascript":
      return javascript();
    case "typescript":
      return javascript({ typescript: true });
    case "python":
      return python();
    case "sql":
      return sql();
    case "json":
      return json();
    case "rust":
      return rust();
    default:
      return [];
  }
}

/** Dark theme matching z8run's slate color palette */
const z8Theme = EditorView.theme(
  {
    "&": {
      backgroundColor: "rgb(30 41 59)", // slate-800
      color: "rgb(226 232 240)", // slate-200
      fontSize: "12px",
      borderRadius: "0 0 6px 6px",
      border: "1px solid rgb(51 65 85)", // slate-700
      borderTop: "none",
    },
    ".cm-content": {
      fontFamily:
        'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
      padding: "8px 0",
      caretColor: "rgb(129 140 248)", // indigo-400
    },
    ".cm-cursor": {
      borderLeftColor: "rgb(129 140 248)",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "rgb(129 140 248)",
    },
    ".cm-activeLine": {
      backgroundColor: "rgba(51, 65, 85, 0.5)", // slate-700/50
    },
    ".cm-selectionBackground": {
      backgroundColor: "rgba(99, 102, 241, 0.3) !important", // indigo-500/30
    },
    "&.cm-focused .cm-selectionBackground": {
      backgroundColor: "rgba(99, 102, 241, 0.3) !important",
    },
    ".cm-gutters": {
      backgroundColor: "rgb(30 41 59)", // slate-800
      color: "rgb(100 116 139)", // slate-500
      border: "none",
      borderRight: "1px solid rgb(51 65 85)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgba(51, 65, 85, 0.5)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      padding: "0 8px 0 4px",
      fontSize: "10px",
    },
    ".cm-scroller": {
      overflow: "auto",
    },
    ".cm-matchingBracket": {
      backgroundColor: "rgba(99, 102, 241, 0.25)",
      color: "rgb(199 210 254) !important", // indigo-200
    },
  },
  { dark: true },
);

/** Fullscreen theme — same palette but with larger font and no border-radius */
const z8ThemeFullscreen = EditorView.theme(
  {
    "&": {
      backgroundColor: "rgb(30 41 59)",
      color: "rgb(226 232 240)",
      fontSize: "14px",
      borderRadius: "0",
      border: "none",
    },
    ".cm-content": {
      fontFamily:
        'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
      padding: "12px 0",
      caretColor: "rgb(129 140 248)",
    },
    ".cm-cursor": {
      borderLeftColor: "rgb(129 140 248)",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "rgb(129 140 248)",
    },
    ".cm-activeLine": {
      backgroundColor: "rgba(51, 65, 85, 0.5)",
    },
    ".cm-selectionBackground": {
      backgroundColor: "rgba(99, 102, 241, 0.3) !important",
    },
    "&.cm-focused .cm-selectionBackground": {
      backgroundColor: "rgba(99, 102, 241, 0.3) !important",
    },
    ".cm-gutters": {
      backgroundColor: "rgb(15 23 42)", // slate-950
      color: "rgb(100 116 139)",
      border: "none",
      borderRight: "1px solid rgb(51 65 85)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgba(51, 65, 85, 0.5)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      padding: "0 12px 0 8px",
      fontSize: "12px",
    },
    ".cm-scroller": {
      overflow: "auto",
    },
    ".cm-matchingBracket": {
      backgroundColor: "rgba(99, 102, 241, 0.25)",
      color: "rgb(199 210 254) !important",
    },
  },
  { dark: true },
);

/* ─── SVG icon paths ─── */
const ExpandIcon = () => (
  <svg
    viewBox="0 0 16 16"
    fill="none"
    className="w-3.5 h-3.5"
    aria-hidden="true"
  >
    <path
      d="M6 1H1v5M15 10v5h-5M1 1l5.5 5.5M15 15l-5.5-5.5"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const ShrinkIcon = () => (
  <svg viewBox="0 0 16 16" fill="none" className="w-4 h-4" aria-hidden="true">
    <path
      d="M1 6h5V1M10 15v-5h5M6 6L0.5 0.5M10 10l5.5 5.5"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const BoltIcon = () => (
  <svg
    viewBox="0 0 16 16"
    fill="none"
    className="w-3.5 h-3.5 text-slate-500"
    aria-hidden="true"
  >
    <path
      d="M4 1L1 8h3l-1 7 9-10H8l2-5H4z"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

/* ─── Language selector bar (shared between inline and fullscreen) ─── */
function LanguageBar({
  language,
  code,
  onLanguageChange,
  rightContent,
  className,
}: {
  language: CodeLanguage;
  code: string;
  onLanguageChange: (lang: CodeLanguage) => void;
  rightContent?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={`flex items-center justify-between px-2 py-1.5 ${className ?? ""}`}
      style={{ backgroundColor: "rgb(15 23 42)" }}
    >
      <div className="flex items-center gap-2">
        <BoltIcon />
        <select
          value={language}
          onChange={(e) => onLanguageChange(e.target.value as CodeLanguage)}
          className="bg-transparent text-[11px] text-slate-300 border-none outline-none
            cursor-pointer hover:text-white transition-colors appearance-none pr-4"
          style={{
            backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath d='M3 5l3 3 3-3' fill='none' stroke='%2394a3b8' stroke-width='1.5'/%3E%3C/svg%3E")`,
            backgroundRepeat: "no-repeat",
            backgroundPosition: "right center",
          }}
        >
          {CODE_LANGUAGES.map((l) => (
            <option
              key={l.value}
              value={l.value}
              className="bg-slate-800 text-slate-200"
            >
              {l.label}
            </option>
          ))}
        </select>
      </div>

      <div className="flex items-center gap-1">
        {/* Auto-detect button */}
        <button
          type="button"
          onClick={() => {
            const detected = detectLanguage(code);
            if (detected !== language) {
              onLanguageChange(detected);
            }
          }}
          className="text-[10px] text-slate-600 hover:text-slate-400 transition-colors px-1.5 py-0.5
            hover:bg-slate-800 rounded"
          title="Auto-detect language"
        >
          detect
        </button>
        {rightContent}
      </div>
    </div>
  );
}

/* ─── Fullscreen modal overlay ─── */
function FullscreenEditor({
  code,
  language,
  onCodeChange,
  onLanguageChange,
  onClose,
  placeholder,
  nodeLabel,
}: {
  code: string;
  language: CodeLanguage;
  onCodeChange: (code: string) => void;
  onLanguageChange: (lang: CodeLanguage) => void;
  onClose: () => void;
  placeholder: string;
  nodeLabel?: string;
}) {
  // Close on Escape key
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  const extensions = useMemo(() => {
    const langExt = getLanguageExtension(language);
    return [
      ...(Array.isArray(langExt) ? langExt : [langExt]),
      EditorView.lineWrapping,
    ];
  }, [language]);

  const handleChange = useCallback(
    (val: string) => onCodeChange(val),
    [onCodeChange],
  );

  // Count lines and characters for the status bar
  const lineCount = code.split("\n").length;
  const charCount = code.length;

  return createPortal(
    <div
      className="fixed inset-0 z-[9999] flex flex-col"
      style={{ backgroundColor: "rgba(2, 6, 23, 0.92)" }}
    >
      {/* Top bar */}
      <div
        className="flex items-center justify-between px-4 py-2 border-b border-slate-700"
        style={{ backgroundColor: "rgb(15 23 42)" }}
      >
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <svg
              viewBox="0 0 16 16"
              fill="none"
              className="w-4 h-4 text-indigo-400"
              aria-hidden="true"
            >
              <path
                d="M5 12l-3-3 3-3M11 4l3 3-3 3"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            <span className="text-sm font-medium text-slate-200">
              {nodeLabel ? `${nodeLabel} — Code Editor` : "Code Editor"}
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onClose}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-slate-400 hover:text-white
              bg-slate-800 hover:bg-slate-700 rounded-md transition-colors border border-slate-700"
          >
            <ShrinkIcon />
            <span>Minimize</span>
            <kbd className="ml-1 text-[10px] text-slate-600 bg-slate-900 px-1 py-0.5 rounded">
              Esc
            </kbd>
          </button>
        </div>
      </div>

      {/* Language bar */}
      <LanguageBar
        language={language}
        code={code}
        onLanguageChange={onLanguageChange}
        className="border-b border-slate-800 px-4"
      />

      {/* Editor area — fills remaining space */}
      <div className="flex-1 overflow-hidden">
        <CodeMirror
          value={code}
          onChange={handleChange}
          extensions={extensions}
          theme={z8ThemeFullscreen}
          placeholder={placeholder}
          height="100%"
          basicSetup={{
            lineNumbers: true,
            highlightActiveLine: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: false,
            foldGutter: true,
            highlightSelectionMatches: true,
            indentOnInput: true,
            searchKeymap: true,
          }}
        />
      </div>

      {/* Status bar */}
      <div
        className="flex items-center justify-between px-4 py-1.5 border-t border-slate-700 text-[10px] text-slate-500"
        style={{ backgroundColor: "rgb(15 23 42)" }}
      >
        <div className="flex items-center gap-4">
          <span>
            {CODE_LANGUAGES.find((l) => l.value === language)?.label ??
              language}
          </span>
          <span>
            {lineCount} {lineCount === 1 ? "line" : "lines"}
          </span>
          <span>{charCount} chars</span>
        </div>
        <span className="text-slate-600">Ctrl+F to search · Esc to close</span>
      </div>
    </div>,
    document.body,
  );
}

/* ─── Main exported component ─── */

interface CodeEditorProps {
  code: string;
  language: CodeLanguage;
  onCodeChange: (code: string) => void;
  onLanguageChange: (lang: CodeLanguage) => void;
  minHeight?: string;
  maxHeight?: string;
  placeholder?: string;
  /** Optional node label shown in the fullscreen header */
  nodeLabel?: string;
}

export function CodeEditor({
  code,
  language,
  onCodeChange,
  onLanguageChange,
  minHeight = "120px",
  maxHeight = "300px",
  placeholder = "// Write your code here...",
  nodeLabel,
}: CodeEditorProps) {
  const [isFullscreen, setIsFullscreen] = useState(false);

  const extensions = useMemo(() => {
    const langExt = getLanguageExtension(language);
    return [
      ...(Array.isArray(langExt) ? langExt : [langExt]),
      EditorView.lineWrapping,
      EditorView.theme({
        ".cm-scroller": {
          minHeight,
          maxHeight,
        },
      }),
    ];
  }, [language, minHeight, maxHeight]);

  const handleChange = useCallback(
    (val: string) => {
      onCodeChange(val);
    },
    [onCodeChange],
  );

  return (
    <>
      <div className="rounded-md overflow-hidden">
        {/* Language selector bar with expand button */}
        <LanguageBar
          language={language}
          code={code}
          onLanguageChange={onLanguageChange}
          className="border border-slate-700 rounded-t-md"
          rightContent={
            <button
              type="button"
              onClick={() => setIsFullscreen(true)}
              className="text-slate-600 hover:text-slate-300 transition-colors p-1
                hover:bg-slate-800 rounded"
              title="Expand editor"
            >
              <ExpandIcon />
            </button>
          }
        />

        {/* CodeMirror editor */}
        <CodeMirror
          value={code}
          onChange={handleChange}
          extensions={extensions}
          theme={z8Theme}
          placeholder={placeholder}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLine: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: false,
            foldGutter: false,
            highlightSelectionMatches: true,
            indentOnInput: true,
          }}
        />
      </div>

      {/* Fullscreen modal */}
      {isFullscreen && (
        <FullscreenEditor
          code={code}
          language={language}
          onCodeChange={onCodeChange}
          onLanguageChange={onLanguageChange}
          onClose={() => setIsFullscreen(false)}
          placeholder={placeholder}
          nodeLabel={nodeLabel}
        />
      )}
    </>
  );
}
