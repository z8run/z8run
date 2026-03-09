import { vaultApi } from "@/api/vault";
import { useVaultKeys } from "@/hooks/useVaultKeys";
import { generateRandomSecret } from "@/lib/crypto";
import { inputClass as baseInputClass } from "@/lib/styles";
import {
  Check,
  ChevronDown,
  Dices,
  Eye,
  EyeOff,
  KeyRound,
  Loader2,
  Plus,
} from "lucide-react";
import { useEffect, useState } from "react";

interface VaultCredentialFieldProps {
  /** Current value — either "vault:key-name" or a raw string */
  value: string;
  /** Called with "vault:key-name" or raw string */
  onChange: (value: string) => void;
  /** Placeholder for manual input */
  placeholder?: string;
  /** Label hint for the vault key (e.g. "openai-api-key") */
  suggestedKeyName?: string;
}

type Mode = "vault" | "manual";

export function VaultCredentialField({
  value,
  onChange,
  placeholder = "Enter value or select from Vault",
  suggestedKeyName = "",
}: VaultCredentialFieldProps) {
  const isVaultRef = value.startsWith("vault:");
  const [mode, setMode] = useState<Mode>(isVaultRef ? "vault" : "manual");
  const {
    keys: vaultKeys,
    loading,
    refetch: fetchKeys,
  } = useVaultKeys({
    fetchOnMount: false,
  });
  const [showDropdown, setShowDropdown] = useState(false);
  const [showValue, setShowValue] = useState(false);
  const [showNewForm, setShowNewForm] = useState(false);
  const [newKeyName, setNewKeyName] = useState(suggestedKeyName);
  const [newKeyValue, setNewKeyValue] = useState("");
  const [saving, setSaving] = useState(false);

  const selectedKey = isVaultRef ? value.slice(6) : "";

  useEffect(() => {
    if (mode === "vault") {
      fetchKeys();
    }
  }, [mode, fetchKeys]);

  const selectVaultKey = (key: string) => {
    onChange(`vault:${key}`);
    setShowDropdown(false);
  };

  const saveNewKey = async () => {
    if (!newKeyName.trim() || !newKeyValue.trim()) return;
    setSaving(true);
    try {
      await vaultApi.store(newKeyName.trim(), newKeyValue.trim());
      await fetchKeys();
      onChange(`vault:${newKeyName.trim()}`);
      setNewKeyName("");
      setNewKeyValue("");
      setShowNewForm(false);
    } catch {
      // Error handled silently — user sees the key didn't appear
    } finally {
      setSaving(false);
    }
  };

  const switchToManual = () => {
    setMode("manual");
    if (isVaultRef) onChange("");
    setShowDropdown(false);
  };

  const switchToVault = () => {
    setMode("vault");
    setShowDropdown(true);
  };

  const inputClass = `${baseInputClass} font-mono transition-colors`;

  return (
    <div className="space-y-1.5">
      {/* Mode toggle */}
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={switchToVault}
          className={`flex items-center gap-1 px-2 py-0.5 rounded text-[10px] transition-colors ${
            mode === "vault"
              ? "bg-amber-900/40 text-amber-400 border border-amber-700/50"
              : "text-slate-500 hover:text-slate-300 border border-transparent"
          }`}
        >
          <KeyRound size={10} />
          Vault
        </button>
        <button
          type="button"
          onClick={switchToManual}
          className={`flex items-center gap-1 px-2 py-0.5 rounded text-[10px] transition-colors ${
            mode === "manual"
              ? "bg-slate-700 text-slate-200 border border-slate-600"
              : "text-slate-500 hover:text-slate-300 border border-transparent"
          }`}
        >
          Manual
        </button>
      </div>

      {mode === "vault" ? (
        <div className="relative">
          {/* Selected vault key display */}
          <button
            type="button"
            onClick={() => setShowDropdown(!showDropdown)}
            className={`${inputClass} flex items-center justify-between cursor-pointer text-left`}
          >
            <span className={selectedKey ? "text-amber-400" : "text-slate-500"}>
              {selectedKey ? (
                <span className="flex items-center gap-1.5">
                  <KeyRound size={10} />
                  {selectedKey}
                </span>
              ) : (
                "Select credential..."
              )}
            </span>
            {loading ? (
              <Loader2 size={12} className="text-slate-500 animate-spin" />
            ) : (
              <ChevronDown size={12} className="text-slate-500" />
            )}
          </button>

          {/* Dropdown */}
          {showDropdown && (
            <div className="absolute z-50 mt-1 w-full bg-slate-800 border border-slate-700 rounded-md shadow-xl max-h-48 overflow-y-auto">
              {vaultKeys.length === 0 && !loading && (
                <div className="px-3 py-2 text-[10px] text-slate-500 italic">
                  No credentials stored yet
                </div>
              )}
              {vaultKeys.map((key) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => selectVaultKey(key)}
                  className="w-full px-3 py-1.5 text-left text-xs text-slate-200 hover:bg-slate-700
                    flex items-center gap-2 transition-colors"
                >
                  <KeyRound size={10} className="text-amber-400 shrink-0" />
                  <span className="truncate font-mono">{key}</span>
                  {selectedKey === key && (
                    <Check
                      size={10}
                      className="text-emerald-400 ml-auto shrink-0"
                    />
                  )}
                </button>
              ))}

              {/* Create new credential */}
              <div className="border-t border-slate-700">
                {!showNewForm ? (
                  <button
                    type="button"
                    onClick={() => setShowNewForm(true)}
                    className="w-full px-3 py-1.5 text-left text-xs text-z8-400 hover:bg-slate-700
                      flex items-center gap-2 transition-colors"
                  >
                    <Plus size={10} />
                    <span>New credential</span>
                  </button>
                ) : (
                  <div className="p-2 space-y-1.5">
                    <input
                      type="text"
                      value={newKeyName}
                      onChange={(e) => setNewKeyName(e.target.value)}
                      placeholder="Key name (e.g. openai-api-key)"
                      className="w-full bg-slate-900 border border-slate-600 rounded px-2 py-1
                        text-[11px] text-slate-200 font-mono focus:outline-none focus:border-z8-500"
                    />
                    <div className="flex gap-1">
                      <input
                        type="password"
                        value={newKeyValue}
                        onChange={(e) => setNewKeyValue(e.target.value)}
                        placeholder="Secret value"
                        className="flex-1 bg-slate-900 border border-slate-600 rounded px-2 py-1
                          text-[11px] text-slate-200 font-mono focus:outline-none focus:border-z8-500"
                      />
                      <button
                        type="button"
                        onClick={() => setNewKeyValue(generateRandomSecret())}
                        className="px-2 py-1 bg-slate-700 hover:bg-slate-600 text-slate-300
                          text-[10px] rounded transition-colors flex items-center gap-1 shrink-0"
                        title="Generate random 256-bit secret"
                      >
                        <Dices size={10} />
                        Generate
                      </button>
                    </div>
                    <div className="flex gap-1">
                      <button
                        type="button"
                        onClick={saveNewKey}
                        disabled={
                          saving || !newKeyName.trim() || !newKeyValue.trim()
                        }
                        className="flex-1 px-2 py-1 bg-z8-600 hover:bg-z8-500 text-white text-[10px]
                          font-medium rounded transition-colors disabled:opacity-40 flex items-center justify-center gap-1"
                      >
                        {saving ? (
                          <Loader2 size={10} className="animate-spin" />
                        ) : (
                          <Plus size={10} />
                        )}
                        Save
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setShowNewForm(false);
                          setNewKeyName("");
                          setNewKeyValue("");
                        }}
                        className="px-2 py-1 text-slate-400 hover:text-slate-200 text-[10px] transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      ) : (
        /* Manual input */
        <div className="relative">
          <input
            type={showValue ? "text" : "password"}
            value={isVaultRef ? "" : value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={placeholder}
            className={inputClass}
          />
          <button
            type="button"
            onClick={() => setShowValue(!showValue)}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition-colors"
          >
            {showValue ? <EyeOff size={12} /> : <Eye size={12} />}
          </button>
        </div>
      )}

      {/* Hint */}
      {mode === "vault" && (
        <div className="text-[9px] text-slate-600">
          Credentials are encrypted with AES-256-GCM. Resolved at execution
          time.
        </div>
      )}
    </div>
  );
}
