let timer: any
let previousResolve: ((v: undefined) => void) | null = null

export function transitionVolume(
  audio: HTMLAudioElement,
  volume: number,
  target: boolean = true,
  lengthen: boolean = false
): Promise<undefined> {
  clearInterval(timer)
  if (previousResolve) {
    previousResolve(undefined)
    previousResolve = null
  }

  const playVolume = lengthen ? 40 : 20
  const pauseVolume = lengthen ? 30 : 20

  return new Promise((resolve) => {
    previousResolve = resolve

    if (target) {
      timer = setInterval(() => {
        audio.volume = Math.min(audio.volume + volume / playVolume, volume)
        if (audio.volume >= volume) {
          clearInterval(timer)
          previousResolve = null
          resolve(undefined)
        }
      }, 50)
      return
    }

    timer = setInterval(() => {
      audio.volume = Math.max(audio.volume - volume / pauseVolume, 0)
      if (audio.volume <= 0) {
        clearInterval(timer)
        audio.volume = volume
        previousResolve = null
        resolve(undefined)
      }
    }, 50)
  })
}
