export interface LyricLine {
  time: number       // start time in ms
  duration: number   // duration in ms
  text: string
  translation?: string
  romanization?: string
  isYrc?: boolean
  yrcWords?: YrcWord[]
}

export interface YrcWord {
  time: number
  duration: number
  text: string
}

export interface ParsedLyrics {
  lines: LyricLine[]
  hasTranslation: boolean
  hasRomanization: boolean
}

export function parseLrc(lrcContent: string): ParsedLyrics {
  if (!lrcContent) return { lines: [], hasTranslation: false, hasRomanization: false }

  const lines: LyricLine[] = []
  const rawLines = lrcContent.replace(/\\n/g, '\n').split('\n')

  for (const line of rawLines) {
    const trimmed = line.trim()
    if (!trimmed) continue

    // Standard LRC: [MM:SS.xx]text
    const match = trimmed.match(/^\[(\d{2}):(\d{2})(?:\.(\d{1,3}))?\](.*)/)
    if (match) {
      const min = parseInt(match[1], 10)
      const sec = parseInt(match[2], 10)
      const ms = match[3] ? parseInt(match[3].padEnd(3, '0'), 10) : 0
      const time = min * 60000 + sec * 1000 + ms
      const text = match[4] || ''
      if (text) {
        lines.push({ time, duration: 0, text, isYrc: false })
      }
      continue
    }

    // Enhanced LRC: [startMs,durationMs]text
    const enhancedMatch = trimmed.match(/^\[(\d+),(\d+)\](.*)/)
    if (enhancedMatch) {
      const time = parseInt(enhancedMatch[1], 10)
      const duration = parseInt(enhancedMatch[2], 10)
      const rawText = enhancedMatch[3] || ''
      // Extract pure text (remove timing markers)
      const text = rawText.replace(/\(\d+,\d+,\d+\)/g, '')
      if (text) {
        lines.push({ time, duration, text, isYrc: false })
      }
      continue
    }

    // Metadata tags: [ti:], [ar:], [al:], [offset:], etc.
    if (/^\[[a-z]+:/i.test(trimmed)) continue
  }

  // Sort by time
  lines.sort((a, b) => a.time - b.time)

  // Calculate durations (gap to next line)
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].duration === 0) {
      const nextTime = i + 1 < lines.length ? lines[i + 1].time : lines[i].time + 5000
      lines[i].duration = nextTime - lines[i].time
    }
  }

  return { lines, hasTranslation: false, hasRomanization: false }
}

export function mergeTranslation(mainLines: LyricLine[], translationLines: LyricLine[]): LyricLine[] {
  const result = mainLines.map(line => ({ ...line }))
  for (const tLine of translationLines) {
    const idx = result.findIndex(l => Math.abs(l.time - tLine.time) < 200)
    if (idx !== -1) {
      result[idx].translation = tLine.text
    }
  }
  return result
}

export function findCurrentLine(lines: LyricLine[], currentTimeMs: number): number {
  if (!lines.length) return -1
  for (let i = lines.length - 1; i >= 0; i--) {
    if (currentTimeMs >= lines[i].time) return i
  }
  return -1
}
