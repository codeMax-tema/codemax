import { create } from 'zustand';

export type NotificationKind = 'info' | 'error';

export interface NotificationItem {
  id: string;
  kind: NotificationKind;
  title: string;
  description?: string;
}

interface NotificationState {
  notifications: NotificationItem[];
  push: (notification: Omit<NotificationItem, 'id'>) => void;
  dismiss: (id: string) => void;
}

export const useNotificationStore = create<NotificationState>((set) => ({
  notifications: [],
  push: (notification) =>
    set((state) => ({
      notifications: [
        ...state.notifications,
        {
          ...notification,
          id: crypto.randomUUID(),
        },
      ],
    })),
  dismiss: (id) =>
    set((state) => ({
      notifications: state.notifications.filter((notification) => notification.id !== id),
    })),
}));

