/**
 * 性能监控器
 * 监控 FPS 并在性能不足时触发降级
 */
export class PerformanceMonitor {
  private fps: number = 60
  private lowFpsCount: number = 0
  private lastTime: number = performance.now()
  private frameCount: number = 0
  private lastFpsUpdate: number = performance.now()

  // 配置
  private readonly fpsThreshold: number = 20 // 低 FPS 阈值
  private readonly lowFpsFrames: number = 300 // 连续低帧数阈值（约 5 秒 @ 60fps）
  private readonly degradeCallback: () => void
  private readonly enabled: boolean

  constructor(options: {
    fpsThreshold?: number
    lowFpsFrames?: number
    onDegrade: () => void
    enabled?: boolean
  }) {
    this.fpsThreshold = options.fpsThreshold ?? 20
    this.lowFpsFrames = options.lowFpsFrames ?? 300
    this.degradeCallback = options.onDegrade
    this.enabled = options.enabled ?? true
  }

  /**
   * 在每帧调用此方法以更新性能数据
   * @returns 'ok' | 'degrade' | 'disabled'
   */
  tick(): 'ok' | 'degrade' | 'disabled' {
    if (!this.enabled) return 'disabled'

    const now = performance.now()
    const delta = now - this.lastTime
    this.lastTime = now

    // 计算当前 FPS
    this.frameCount++

    // 每 500ms 更新一次 FPS
    if (now - this.lastFpsUpdate >= 500) {
      this.fps = (this.frameCount * 1000) / (now - this.lastFpsUpdate)
      this.frameCount = 0
      this.lastFpsUpdate = now
    }

    // 检查是否低于阈值
    if (this.fps < this.fpsThreshold) {
      this.lowFpsCount++
      if (this.lowFpsCount >= this.lowFpsFrames) {
        return 'degrade' // 触发降级
      }
    } else {
      this.lowFpsCount = 0
    }

    return 'ok'
  }

  /**
   * 获取当前 FPS
   */
  getFPS(): number {
    return this.fps
  }

  /**
   * 重置计数器（用于降级后重新开始监控）
   */
  reset(): void {
    this.lowFpsCount = 0
    this.frameCount = 0
    this.lastFpsUpdate = performance.now()
    this.lastTime = performance.now()
  }

  /**
   * 检查是否处于低性能状态
   */
  isLowPerformance(): boolean {
    return this.lowFpsCount > this.lowFpsFrames * 0.5
  }
}

/**
 * 性能降级管理器
 * 管理配置降级和用户提示
 */
export class PerformanceDegrader {
  private hasDegraded: boolean = false
  private monitor: PerformanceMonitor | null = null
  private rafId: number | null = null
  private onDegradeCallback?: (newConfig: any) => void

  constructor() {}

  /**
   * 启动性能监控
   */
  start(options: {
    onTick?: (fps: number) => void
    onDegrade: (newConfig: any) => void
    enabled?: boolean
  }) {
    this.onDegradeCallback = options.onDegrade

    this.monitor = new PerformanceMonitor({
      onDegrade: () => this.handleDegrade(),
      enabled: options.enabled ?? true
    })

    const tick = () => {
      if (!this.monitor) return

      const result = this.monitor.tick()

      if (options.onTick) {
        options.onTick(this.monitor.getFPS())
      }

      if (result === 'degrade') {
        this.handleDegrade()
      }

      this.rafId = requestAnimationFrame(tick)
    }

    this.rafId = requestAnimationFrame(tick)
  }

  /**
   * 停止性能监控
   */
  stop() {
    if (this.rafId) {
      cancelAnimationFrame(this.rafId)
      this.rafId = null
    }
    this.monitor = null
  }

  /**
   * 处理性能降级
   */
  private handleDegrade() {
    if (this.hasDegraded || !this.onDegradeCallback) return

    this.hasDegraded = true

    // 生成降级配置
    const degradedConfig = {
      renderScale: 0.3,
      fps: 15,
      audioResponse: false,
      flowSpeed: 0.5,
      staticMode: true,
      preset: 'custom' as const
    }

    this.onDegradeCallback(degradedConfig)

    // 停止监控（避免重复降级）
    this.stop()
  }

  /**
   * 重置降级状态
   */
  reset() {
    this.hasDegraded = false
    this.monitor?.reset()
  }

  /**
   * 是否已经降级
   */
  hasDegradedConfig(): boolean {
    return this.hasDegraded
  }
}
