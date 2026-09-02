const DESKTOP_UPDATE_PLATFORMS = new Set(['linux', 'macos', 'windows'])

export function isDesktopUpdatePlatform(platformName: string | null | undefined): boolean {
  return platformName != null && DESKTOP_UPDATE_PLATFORMS.has(platformName)
}
