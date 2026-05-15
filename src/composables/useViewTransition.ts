export const supportsViewTransition =
  typeof document !== 'undefined' && 'startViewTransition' in document

/**
 * Wrap a DOM state change in a View Transition.
 * Falls back to running the callback directly if the API is unavailable.
 */
export function withViewTransition(
  callback: () => Promise<void> | void,
): Promise<void> | void {
  if (!supportsViewTransition) {
    return callback()
  }
  const transition = document.startViewTransition(callback)
  return transition.finished.catch(() => {})
}
