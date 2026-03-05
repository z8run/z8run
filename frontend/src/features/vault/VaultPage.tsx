import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { vaultApi } from "@/api/vault";
import {
  Shield,
  Plus,
  Trash2,
  Eye,
  EyeOff,
  Copy,
  LogOut,
  AlertTriangle,
  Loader2,
} from "lucide-react";
import { useAuthStore } from "@/stores/authStore";

export function VaultPage() {
  const navigate = useNavigate();
  const { logout, user } = useAuthStore();

  const [keys, setKeys] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [revealedValues, setRevealedValues] = useState<Record<string, string>>({});
  const [visibleKeys, setVisibleKeys] = useState<Set<string>>(new Set());
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  const loadKeys = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await vaultApi.list();
      setKeys(res.keys);
    } catch (e) {
      console.error("Failed to load vault keys", e);
      setError("Failed to load credentials");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadKeys();
  }, [loadKeys]);

  const handleStore = async () => {
    if (!newKey.trim() || !newValue.trim()) {
      setError("Key and value are required");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await vaultApi.store(newKey.trim(), newValue.trim());
      setNewKey("");
      setNewValue("");
      setShowAddForm(false);
      await loadKeys();
    } catch (e) {
      console.error("Failed to store credential", e);
      setError("Failed to store credential");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (key: string) => {
    setSaving(true);
    setError(null);
    try {
      await vaultApi.delete(key);
      setRevealedValues((prev) => {
        const n = { ...prev };
        delete n[key];
        return n;
      });
      setVisibleKeys((prev) => {
        const n = new Set(prev);
        n.delete(key);
        return n;
      });
      setDeleteTarget(null);
      await loadKeys();
    } catch (e) {
      console.error("Failed to delete credential", e);
      setError("Failed to delete credential");
    } finally {
      setSaving(false);
    }
  };

  const toggleReveal = async (key: string) => {
    if (visibleKeys.has(key)) {
      setVisibleKeys((prev) => {
        const n = new Set(prev);
        n.delete(key);
        return n;
      });
      return;
    }
    try {
      const res = await vaultApi.get(key);
      setRevealedValues((prev) => ({ ...prev, [key]: res.value }));
      setVisibleKeys((prev) => new Set(prev).add(key));
    } catch (e) {
      console.error("Failed to retrieve credential", e);
      setError("Failed to retrieve credential");
    }
  };

  const copyToClipboard = async (key: string) => {
    try {
      let value = revealedValues[key];
      if (!value) {
        const res = await vaultApi.get(key);
        value = res.value;
      }
      await navigator.clipboard.writeText(value);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 2000);
    } catch (e) {
      console.error("Failed to copy", e);
      setError("Failed to copy to clipboard");
    }
  };

  return (
    <div className="min-h-screen bg-slate-950">
      {/* Top bar */}
      <div className="border-b border-slate-800">
        <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-z8-600 flex items-center justify-center">
              <span className="text-xs font-bold text-white">z8</span>
            </div>
            <div>
              <h1 className="text-lg font-semibold text-slate-100">z8run</h1>
              <p className="text-xs text-slate-500">Credential Vault</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            {user && <span className="text-xs text-slate-500">{user.email}</span>}
            <button
              type="button"
              onClick={() => navigate("/")}
              className="px-3 py-2 text-slate-300 hover:bg-slate-800 text-sm rounded-lg transition-colors border border-slate-700"
              title="Back to flows"
            >
              Back
            </button>
            <button
              type="button"
              onClick={logout}
              className="p-2 text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-lg transition-colors"
              title="Sign out"
            >
              <LogOut size={16} />
            </button>
          </div>
        </div>
      </div>

      {/* Content */}
      <div className="max-w-5xl mx-auto px-6 py-8">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-lg bg-slate-800 flex items-center justify-center">
              <Shield size={20} className="text-slate-400" />
            </div>
            <div>
              <h2 className="text-xl font-semibold text-slate-100">
                Credential Vault
              </h2>
              <p className="text-xs text-slate-500 mt-1">
                Securely store and manage API keys and secrets (AES-256-GCM encrypted)
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={() => setShowAddForm(true)}
            className="flex items-center gap-2 px-4 py-2 bg-z8-600 hover:bg-z8-700
              text-white text-sm font-medium rounded-lg transition-colors"
          >
            <Plus size={16} />
            Add Secret
          </button>
        </div>

        {/* Error banner */}
        {error && (
          <div className="mb-6 bg-red-900/20 border border-red-700 rounded-lg p-4 flex items-start gap-3">
            <AlertTriangle size={16} className="text-red-400 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm text-red-300">{error}</p>
            </div>
          </div>
        )}

        {/* Add credential form */}
        {showAddForm && (
          <div className="mb-6 bg-slate-900 border border-slate-700 rounded-lg p-6">
            <h3 className="text-sm font-medium text-slate-200 mb-4">
              Add New Credential
            </h3>
            <div className="space-y-4">
              <div>
                <label className="block text-xs font-medium text-slate-400 mb-2">
                  Key Name
                </label>
                <input
                  type="text"
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                  onKeyDown={(e) =>
                    e.key === "Enter" && !e.shiftKey && handleStore()
                  }
                  placeholder="e.g., openai_api_key"
                  className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-2
                    text-sm text-slate-200 placeholder-slate-500 focus:outline-none
                    focus:border-z8-500"
                  autoFocus
                />
              </div>
              <div>
                <label className="block text-xs font-medium text-slate-400 mb-2">
                  Secret Value
                </label>
                <textarea
                  value={newValue}
                  onChange={(e) => setNewValue(e.target.value)}
                  placeholder="Your API key or secret..."
                  className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-2
                    text-sm text-slate-200 placeholder-slate-500 focus:outline-none
                    focus:border-z8-500 font-mono resize-none"
                  rows={3}
                />
              </div>
              <div className="flex gap-3 justify-end">
                <button
                  type="button"
                  onClick={() => {
                    setShowAddForm(false);
                    setNewKey("");
                    setNewValue("");
                    setError(null);
                  }}
                  className="px-4 py-2 text-slate-400 hover:bg-slate-800 text-sm
                    rounded-md transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={handleStore}
                  disabled={saving || !newKey.trim() || !newValue.trim()}
                  className="flex items-center gap-2 px-4 py-2 bg-z8-600 hover:bg-z8-700
                    disabled:bg-slate-700 disabled:cursor-not-allowed text-white text-sm
                    rounded-md transition-colors"
                >
                  {saving && <Loader2 size={14} className="animate-spin" />}
                  Save
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Credentials list */}
        {loading ? (
          <div className="text-center text-slate-500 py-20">
            <Loader2 size={24} className="animate-spin mx-auto mb-3" />
            <p>Loading credentials...</p>
          </div>
        ) : keys.length === 0 ? (
          <div className="text-center py-20">
            <div className="w-16 h-16 rounded-full bg-slate-800 flex items-center justify-center mx-auto mb-4">
              <Shield size={24} className="text-slate-600" />
            </div>
            <h3 className="text-lg font-medium text-slate-300 mb-2">
              No secrets yet
            </h3>
            <p className="text-sm text-slate-500 mb-4">
              Add your first API key or secret
            </p>
            <button
              type="button"
              onClick={() => setShowAddForm(true)}
              className="inline-flex items-center gap-2 px-4 py-2 bg-z8-600 hover:bg-z8-700
                text-white text-sm font-medium rounded-lg transition-colors"
            >
              <Plus size={14} />
              Add Secret
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            {keys.map((key) => {
              const isVisible = visibleKeys.has(key);
              const value = revealedValues[key];
              const isCopied = copiedKey === key;

              return (
                <div
                  key={key}
                  className="bg-slate-900 border border-slate-800 hover:border-slate-700
                    rounded-lg p-4 transition-colors group"
                >
                  <div className="flex items-center justify-between">
                    <div className="flex-1 min-w-0">
                      <h4 className="text-sm font-medium text-slate-200 group-hover:text-white break-all">
                        {key}
                      </h4>
                      <div className="mt-2">
                        {isVisible && value ? (
                          <code className="text-xs bg-slate-800 text-slate-300 px-2 py-1 rounded
                            block break-all max-w-lg overflow-x-auto font-mono">
                            {value}
                          </code>
                        ) : (
                          <code className="text-xs bg-slate-800 text-slate-600 px-2 py-1 rounded
                            block font-mono">
                            ••••••••••••••••
                          </code>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 ml-4 flex-shrink-0">
                      <button
                        type="button"
                        onClick={() => toggleReveal(key)}
                        className="p-2 text-slate-400 hover:text-slate-200 hover:bg-slate-800
                          rounded transition-colors opacity-0 group-hover:opacity-100"
                        title={isVisible ? "Hide" : "Reveal"}
                      >
                        {isVisible ? (
                          <EyeOff size={16} />
                        ) : (
                          <Eye size={16} />
                        )}
                      </button>
                      <button
                        type="button"
                        onClick={() => copyToClipboard(key)}
                        className="p-2 text-slate-400 hover:text-slate-200 hover:bg-slate-800
                          rounded transition-colors opacity-0 group-hover:opacity-100"
                        title="Copy to clipboard"
                      >
                        <Copy
                          size={16}
                          className={
                            isCopied ? "text-green-400" : "text-slate-400"
                          }
                        />
                      </button>
                      <button
                        type="button"
                        onClick={() => setDeleteTarget(key)}
                        className="p-2 text-slate-400 hover:text-red-400 hover:bg-red-900/30
                          rounded transition-colors opacity-0 group-hover:opacity-100"
                        title="Delete"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Delete confirmation modal */}
      {deleteTarget && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="bg-slate-900 border border-slate-700 rounded-lg p-6 max-w-sm w-full mx-4 shadow-xl">
            <div className="flex items-center gap-3 mb-4">
              <div className="w-10 h-10 rounded-full bg-red-900/30 flex items-center justify-center">
                <AlertTriangle size={20} className="text-red-400" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-slate-200">
                  Delete Credential
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">
                  This action cannot be undone
                </p>
              </div>
            </div>
            <p className="text-sm text-slate-300 mb-6">
              Are you sure you want to delete{" "}
              <span className="font-medium text-white">"{deleteTarget}"</span>?
            </p>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                className="px-4 py-2 text-sm text-slate-400 hover:bg-slate-800 rounded-md transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => handleDelete(deleteTarget)}
                disabled={saving}
                className="flex items-center gap-2 px-4 py-2 text-sm bg-red-600 hover:bg-red-700
                  disabled:bg-slate-700 disabled:cursor-not-allowed text-white rounded-md transition-colors"
              >
                {saving && <Loader2 size={14} className="animate-spin" />}
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
