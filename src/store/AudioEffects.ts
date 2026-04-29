import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface AudioEffectsState {
  bassBoost: {
    enabled: boolean
    gain: number
  }
  surround: {
    enabled: boolean
    mode: 'off' | 'small' | 'medium' | 'large'
  }
  balance: {
    enabled: boolean
    value: number
  }
}

export const useAudioEffectsStore = defineStore(
  'audioEffects',
  () => {
    const bassBoost = ref({
      enabled: false,
      gain: 0
    })

    const surround = ref({
      enabled: false,
      mode: 'off' as const
    })

    const balance = ref({
      enabled: true,
      value: 0
    })

    const resetEffects = () => {
      bassBoost.value = { enabled: false, gain: 0 }
      surround.value = { enabled: false, mode: 'off' }
      balance.value = { enabled: true, value: 0 }
    }

    return {
      bassBoost,
      surround,
      balance,
      resetEffects
    }
  },
  {
    persist: true
  }
)
