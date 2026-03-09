import { vaultApi } from "@/api/vault";
import { useCallback, useEffect, useState } from "react";

/**
 * Custom hook for fetching and managing vault credential keys.
 *
 * Eliminates duplicated vault API state management between
 * VaultPage.tsx and VaultCredentialField.tsx.
 */
export function useVaultKeys(options?: { fetchOnMount?: boolean }) {
  const fetchOnMount = options?.fetchOnMount ?? true;

  const [keys, setKeys] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchKeys = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await vaultApi.list();
      setKeys(res.keys ?? []);
    } catch (e) {
      console.error("Failed to load vault keys", e);
      setKeys([]);
      setError("Failed to load credentials");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (fetchOnMount) {
      fetchKeys();
    }
  }, [fetchOnMount, fetchKeys]);

  return { keys, loading, error, refetch: fetchKeys, setError };
}
