import { AlertTriangle, Loader2 } from "lucide-react";

interface ConfirmDialogProps {
  /** Whether the dialog is visible */
  open: boolean;
  /** Title shown in the dialog header */
  title: string;
  /** Subtitle under the title */
  subtitle?: string;
  /** Main message body — can include JSX */
  children: React.ReactNode;
  /** Label for the confirm button */
  confirmLabel?: string;
  /** Whether the confirm action is in progress */
  loading?: boolean;
  /** Visual style of confirm button */
  variant?: "danger" | "primary";
  /** Called when confirm button is clicked */
  onConfirm: () => void;
  /** Called when cancel button is clicked or overlay is clicked */
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  subtitle = "This action cannot be undone",
  children,
  confirmLabel = "Confirm",
  loading = false,
  variant = "danger",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  if (!open) return null;

  const confirmClass =
    variant === "danger"
      ? "bg-red-600 hover:bg-red-700"
      : "bg-z8-600 hover:bg-z8-700";

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-slate-900 border border-slate-700 rounded-lg p-6 max-w-sm w-full mx-4 shadow-xl">
        <div className="flex items-center gap-3 mb-4">
          <div className="w-10 h-10 rounded-full bg-red-900/30 flex items-center justify-center">
            <AlertTriangle size={20} className="text-red-400" />
          </div>
          <div>
            <h3 className="text-sm font-semibold text-slate-200">{title}</h3>
            {subtitle && (
              <p className="text-xs text-slate-400 mt-0.5">{subtitle}</p>
            )}
          </div>
        </div>
        <div className="text-sm text-slate-300 mb-6">{children}</div>
        <div className="flex justify-end gap-3">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-sm text-slate-400 hover:bg-slate-800 rounded-md transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={loading}
            className={`flex items-center gap-2 px-4 py-2 text-sm ${confirmClass}
              disabled:bg-slate-700 disabled:cursor-not-allowed text-white rounded-md transition-colors`}
          >
            {loading && <Loader2 size={14} className="animate-spin" />}
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
