import { invoke } from '@tauri-apps/api/core';

export type InvokeParams = Record<string, unknown>;

export async function invokeCommand<TResponse>(
  command: string,
  params: InvokeParams = {},
): Promise<TResponse> {
  return invoke<TResponse>(command, params);
}

export interface HealthResponse {
  service: string;
  status: 'ok';
  version: string;
}

export function getDesktopHealth() {
  return invokeCommand<HealthResponse>('health');
}

