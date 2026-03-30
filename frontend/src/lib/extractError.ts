/**
 * Extract a human-readable error message from an unknown caught error.
 * Handles ky HTTP errors, standard Errors, strings, and unknowns.
 */
export async function extractErrorMessage(
  err: unknown,
  fallback = "Something went wrong"
): Promise<string> {
  // ky / fetch-style errors with a Response body
  const resErr = err as { response?: Response };
  if (resErr?.response) {
    try {
      const body = await resErr.response.json();
      const msg =
        (body as Record<string, { message?: string }>)?.error?.message ||
        (body as { message?: string })?.message;
      if (msg) return msg;
    } catch {
      // response isn't JSON — fall through
    }
  }
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return fallback;
}
