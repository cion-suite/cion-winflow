import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs } from "@tauri-apps/api/core";

function toError(cause: unknown): Error {
  if (cause instanceof Error) {
    return cause;
  }
  if (typeof cause === "string") {
    return new Error(cause, { cause });
  }
  // A Rust error type arrives as whatever its Serialize produced
  const message = (cause as { message?: unknown } | null)?.message;
  return new Error(
    typeof message === "string" ? message : JSON.stringify(cause),
    { cause },
  );
}

// T is a promise to the compiler, nothing checks it against the Rust signature.
export async function call<T>(command: string, args?: InvokeArgs): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (cause: unknown) {
    throw toError(cause);
  }
}
