/** Shared Tailwind CSS class constants for z8run UI components. */

/** Compact input for config panels and editor UI (text-xs, smaller padding) */
export const inputClass =
  "w-full bg-slate-800 border border-slate-600 rounded px-2 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-z8-500";

/** Standard input for full-page forms (text-sm, more padding) */
export const formInputClass =
  "w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-z8-500";

export const selectClass =
  "w-full bg-slate-800 border border-slate-600 rounded px-2 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-z8-500";

/** Compact label for config panels (10px) */
export const labelClass = "text-[10px] font-medium text-slate-400 block mb-1";

/** Standard label for full-page forms (xs) */
export const formLabelClass = "block text-xs text-slate-400 mb-1.5";

export const textareaClass = `${inputClass} font-mono resize-none`;

export const errorBannerClass =
  "mb-6 bg-red-900/20 border border-red-700 rounded-lg p-4 flex items-start gap-3";
