import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export const APP_READY_EVENT = 'codemax://app-ready';

export interface AppReadyPayload {
  service: string;
  version: string;
}

export function listenAppReady(handler: (payload: AppReadyPayload) => void): Promise<UnlistenFn> {
  return listen<AppReadyPayload>(APP_READY_EVENT, (event) => {
    handler(event.payload);
  });
}

