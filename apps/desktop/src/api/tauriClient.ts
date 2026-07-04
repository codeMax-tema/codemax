import { invoke } from '@tauri-apps/api/core';

import { normalizeIpcError } from '@/api/errors';

export type InvokeParams = Record<string, unknown>;

export async function invokeCommand<TResponse>(
  command: string,
  params: InvokeParams = {},
): Promise<TResponse> {
  try {
    return await invoke<TResponse>(command, params);
  } catch (error) {
    throw normalizeIpcError(error);
  }
}

export interface HealthResponse {
  service: string;
  status: 'ok';
  version: string;
}

export function getDesktopHealth() {
  return invokeCommand<HealthResponse>('health');
}

export interface PingResponse {
  message: string;
}

export function pingDesktop() {
  return invokeCommand<PingResponse>('ping');
}

export function emitAppReady() {
  return invokeCommand<void>('emit_app_ready');
}
