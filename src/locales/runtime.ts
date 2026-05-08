import i18n from '@/locales'
import type { AppLocale } from '@/store/Settings'

export type ResolvedLocale = 'zh-CN' | 'en-US'

export const normalizeLocale = (locale?: string | null): ResolvedLocale => {
  if (locale?.toLowerCase().startsWith('zh')) return 'zh-CN'
  return 'en-US'
}

export const setI18nLocale = (locale: ResolvedLocale) => {
  i18n.global.locale.value = locale
  document.documentElement.lang = locale
}

export const resolveAppLocale = async (language: AppLocale | undefined): Promise<ResolvedLocale> => {
  if (language && language !== 'system') return language

  try {
    const { locale } = await import('@tauri-apps/plugin-os')
    return normalizeLocale(await locale())
  } catch {
    return 'zh-CN'
  }
}
