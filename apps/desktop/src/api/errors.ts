export interface NormalizedError {
  title: string;
  description?: string;
}

export function normalizeIpcError(error: unknown): NormalizedError {
  if (error instanceof Error) {
    return {
      title: error.message,
    };
  }

  if (typeof error === 'string') {
    return {
      title: error,
    };
  }

  if (typeof error === 'object' && error !== null && 'message' in error) {
    return {
      title: String((error as { message: unknown }).message),
    };
  }

  return {
    title: 'Unknown desktop error',
    description: 'The local backend returned an unexpected error shape.',
  };
}

