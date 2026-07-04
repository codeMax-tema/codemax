import { Toast, ToastDescription, ToastProvider, ToastTitle, ToastViewport } from '@/components/ui/toast';
import { useNotificationStore } from '@/state/notificationStore';

export function Toaster() {
  const notifications = useNotificationStore((state) => state.notifications);
  const dismiss = useNotificationStore((state) => state.dismiss);

  return (
    <ToastProvider swipeDirection="right">
      {notifications.map((notification) => (
        <Toast
          key={notification.id}
          open
          variant={notification.kind === 'error' ? 'destructive' : 'default'}
          onOpenChange={(open) => {
            if (!open) {
              dismiss(notification.id);
            }
          }}
        >
          <ToastTitle>{notification.title}</ToastTitle>
          {notification.description ? (
            <ToastDescription>{notification.description}</ToastDescription>
          ) : null}
        </Toast>
      ))}
      <ToastViewport />
    </ToastProvider>
  );
}

