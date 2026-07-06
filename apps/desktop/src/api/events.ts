import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { CommandFinishedEvent, CommandOutputEvent } from '@/types/domain';

export const APP_READY_EVENT = 'codemax://app-ready';
export const COMMAND_OUTPUT_EVENT = 'codemax://command-output';
export const COMMAND_FINISHED_EVENT = 'codemax://command-finished';

export interface AppReadyPayload {
  service: string;
  version: string;
}

export function listenAppReady(handler: (payload: AppReadyPayload) => void): Promise<UnlistenFn> {
  return listen<AppReadyPayload>(APP_READY_EVENT, (event) => {
    handler(event.payload);
  });
}

export function listenCommandOutput(
  handler: (payload: CommandOutputEvent) => void,
): Promise<UnlistenFn> {
  return listen<CommandOutputEvent>(COMMAND_OUTPUT_EVENT, (event) => {
    handler(event.payload);
  });
}

export function listenCommandFinished(
  handler: (payload: CommandFinishedEvent) => void,
): Promise<UnlistenFn> {
  return listen<CommandFinishedEvent>(COMMAND_FINISHED_EVENT, (event) => {
    handler(event.payload);
  });
}

