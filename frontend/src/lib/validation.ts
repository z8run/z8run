/**
 * Tiny, dependency-free runtime validators for the MOST critical API responses.
 *
 * The app otherwise trusts `ky(...).json<T>()` casts blindly. If the backend
 * shape changes or data is corrupted, that surfaces late and confusingly. These
 * hand-written type-guards run at the fetch boundary so a malformed response
 * fails fast with a clear, logged error instead of crashing deep in the UI.
 *
 * This is intentionally NARROW: only the auth response and the editor's
 * flow-load response are covered. A full schema-validation layer (e.g. a
 * generated client, or a validation library applied to every endpoint) is the
 * larger follow-up — see FUNC-010.
 */

import type { AuthResponse } from "@/api/auth";
import type { FlowDetail } from "@/types/flow";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Validate a login/register response before it is trusted and stored.
 * Requires a string `token` and a `user` object carrying the expected fields.
 * Throws a descriptive Error on any mismatch.
 */
export function assertAuthResponse(value: unknown): AuthResponse {
  if (!isRecord(value)) {
    throw new Error("Invalid auth response: expected an object");
  }
  if (typeof value.token !== "string" || value.token.length === 0) {
    throw new Error("Invalid auth response: missing or non-string 'token'");
  }
  const user = value.user;
  if (!isRecord(user)) {
    throw new Error("Invalid auth response: missing 'user' object");
  }
  for (const field of ["id", "email", "username"] as const) {
    if (typeof user[field] !== "string") {
      throw new Error(`Invalid auth response: user.${field} must be a string`);
    }
  }
  if (!Array.isArray(user.roles)) {
    throw new Error("Invalid auth response: user.roles must be an array");
  }
  return value as unknown as AuthResponse;
}

/**
 * Validate the flow-detail response the editor loads onto the canvas.
 * The canvas rendering casts `canvas_nodes` / `canvas_edges` to arrays and maps
 * over them, so a non-array here would crash mid-render. Validate the shape
 * (fields are optional in the payload, but when present must be arrays) and
 * throw a descriptive Error otherwise.
 */
export function assertFlowDetail(value: unknown): FlowDetail {
  if (!isRecord(value)) {
    throw new Error("Invalid flow response: expected an object");
  }
  if (typeof value.id !== "string") {
    throw new Error("Invalid flow response: missing or non-string 'id'");
  }
  if (value.canvas_nodes != null && !Array.isArray(value.canvas_nodes)) {
    throw new Error("Invalid flow response: 'canvas_nodes' must be an array");
  }
  if (value.canvas_edges != null && !Array.isArray(value.canvas_edges)) {
    throw new Error("Invalid flow response: 'canvas_edges' must be an array");
  }
  return value as unknown as FlowDetail;
}
