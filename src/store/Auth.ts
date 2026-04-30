import { defineStore } from 'pinia'
import { ref } from 'vue'
import LogtoClient, { type UserInfoResponse } from '@logto/browser'
import { MessagePlugin } from 'tdesign-vue-next'
import config from '@/config'

const { redirectUri, postLogoutRedirectUri } = config

export const useAuthStore = defineStore('auth', () => {
  const user = ref<UserInfoResponse | null>(null)
  const isAuthenticated = ref(false)
  const loading = ref(false)

  const logtoClient = config.instance as LogtoClient

  const init = async () => {
    try {
      loading.value = true
      await updateUserInfo()
    } catch (error: any) {
      console.error('Failed to init auth:', error)
      if (error?.cause?.status >= 400 && error?.cause?.status < 500) {
        MessagePlugin.error('登录过期，请重新登录')
        forceLogout()
      }
    } finally {
      loading.value = false
    }
  }

  const login = async () => {
    try {
      await logtoClient.signIn(redirectUri)
    } catch (error) {
      console.error('Sign in failed:', error)
    }
  }

  const logout = async () => {
    try {
      await logtoClient.signOut(postLogoutRedirectUri)
      user.value = null
      isAuthenticated.value = false
      logtoClient.clearAccessToken()
      logtoClient.clearAllTokens()
    } catch (error) {
      console.error('Sign out failed:', error)
    }
    MessagePlugin.success('退出成功')
  }

  const forceLogout = async () => {
    logtoClient?.clearAccessToken()
    logtoClient?.clearAllTokens()
    isAuthenticated.value = false
    user.value = null
  }

  const handleCallback = async (callbackUrl: string) => {
    try {
      loading.value = true
      await logtoClient.handleSignInCallback(callbackUrl)
      await updateUserInfo()
      if (isAuthenticated.value) {
        MessagePlugin.success('登录成功')
      } else {
        MessagePlugin.error('登录失败')
      }
    } catch (error) {
      console.error('Callback handling failed:', error)
      MessagePlugin.error('登录回调处理失败')
    } finally {
      loading.value = false
    }
  }

  const updateUserInfo = async () => {
    isAuthenticated.value = await logtoClient.isAuthenticated()
    if (isAuthenticated.value) {
      user.value = await logtoClient.fetchUserInfo()
    } else {
      user.value = null
    }
  }

  return {
    user,
    isAuthenticated,
    loading,
    init,
    login,
    logout,
    handleCallback,
    updateUserInfo,
    forceLogout
  }
}, {
  persist: true
})
