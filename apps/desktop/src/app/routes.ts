export const routes = {
  repository: '/',
  tasks: '/tasks',
  taskDetail: '/tasks/:taskId',
  approvals: '/approvals',
  settings: '/settings',
} as const;

export type AppRoute = (typeof routes)[keyof typeof routes];

