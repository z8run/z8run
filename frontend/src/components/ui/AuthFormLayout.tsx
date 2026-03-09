import { Loader2 } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

interface AuthFormLayoutProps {
  /** Page heading e.g. "Sign in" or "Create account" */
  title: string;
  /** Error message to display (if any) */
  error?: string | null;
  /** Form fields and submit button */
  children: ReactNode;
  /** Submit button props */
  submitLabel: string;
  submitLoadingLabel: string;
  submitIcon: LucideIcon;
  loading: boolean;
  onSubmit: (e: React.FormEvent) => void;
  /** Footer link */
  footerText: string;
  footerLinkText: string;
  footerLinkTo: string;
}

export function AuthFormLayout({
  title,
  error,
  children,
  submitLabel,
  submitLoadingLabel,
  submitIcon: Icon,
  loading,
  onSubmit,
  footerText,
  footerLinkText,
  footerLinkTo,
}: AuthFormLayoutProps) {
  return (
    <div className="min-h-screen bg-slate-950 flex items-center justify-center">
      <div className="w-full max-w-sm mx-4">
        {/* Logo */}
        <div className="flex items-center justify-center gap-3 mb-8">
          <div className="w-10 h-10 rounded-lg bg-z8-600 flex items-center justify-center">
            <span className="text-sm font-bold text-white">z8</span>
          </div>
          <div>
            <h1 className="text-xl font-semibold text-slate-100">z8run</h1>
            <p className="text-xs text-slate-500">Flow Engine</p>
          </div>
        </div>

        {/* Form card */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-6">
          <h2 className="text-lg font-medium text-slate-200 mb-6">{title}</h2>

          {error && (
            <div className="mb-4 p-3 bg-red-900/30 border border-red-800 rounded-md text-sm text-red-300">
              {error}
            </div>
          )}

          <form onSubmit={onSubmit} className="space-y-4">
            {children}
            <button
              type="submit"
              disabled={loading}
              className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-z8-600
                hover:bg-z8-700 text-white text-sm font-medium rounded-lg transition-colors
                disabled:opacity-60"
            >
              {loading ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <Icon size={16} />
              )}
              {loading ? submitLoadingLabel : submitLabel}
            </button>
          </form>

          <p className="mt-4 text-center text-xs text-slate-500">
            {footerText}{" "}
            <Link to={footerLinkTo} className="text-z8-400 hover:text-z8-300">
              {footerLinkText}
            </Link>
          </p>
        </div>
      </div>
    </div>
  );
}
