const COMMON_SUFFIXES = ["1", "12", "123", "1234", "2024", "2025", "2026", "!", "@123"]
const MAX_GENERATED_CANDIDATES = 50_000

export interface DictionaryGenerationOptions {
  uppercase: boolean
  capitalize: boolean
  leetspeak: boolean
  commonSuffixes: boolean
  combineWords: boolean
  includeFilenamePatterns: boolean
}

function capitalizeWord(word: string): string {
  if (word.length === 0) return word
  return word[0].toUpperCase() + word.slice(1).toLowerCase()
}

function leetspeakVariant(word: string): string {
  return word
    .replace(/a/gi, "@")
    .replace(/e/gi, "3")
    .replace(/i/gi, "1")
    .replace(/o/gi, "0")
    .replace(/s/gi, "$")
}

function filenameSeeds(fileName: string): string[] {
  const stem = fileName.replace(/\.[^.]+$/, "")
  return stem
    .split(/[^a-zA-Z0-9]+/)
    .map((part) => part.trim())
    .filter((part) => part.length >= 2)
}

export function buildDictionaryCandidates(
  baseWords: string[],
  fileName: string,
  options: DictionaryGenerationOptions,
): string[] {
  const seen = new Set<string>()
  const result: string[] = []

  const pushCandidate = (candidate: string) => {
    const normalized = candidate.trim()
    if (!normalized || seen.has(normalized) || result.length >= MAX_GENERATED_CANDIDATES) {
      return
    }
    seen.add(normalized)
    result.push(normalized)
  }

  const seeds = [...baseWords]
  if (options.includeFilenamePatterns) {
    seeds.push(...filenameSeeds(fileName))
  }

  for (const seed of seeds) {
    pushCandidate(seed)
    if (options.uppercase) pushCandidate(seed.toUpperCase())
    if (options.capitalize) pushCandidate(capitalizeWord(seed))
    if (options.leetspeak) pushCandidate(leetspeakVariant(seed))
    if (options.commonSuffixes) {
      for (const suffix of COMMON_SUFFIXES) {
        pushCandidate(`${seed}${suffix}`)
      }
    }
  }

  if (options.combineWords) {
    const combineSource = result.slice(0, Math.min(result.length, 200))
    for (let i = 0; i < combineSource.length; i += 1) {
      for (let j = 0; j < combineSource.length; j += 1) {
        if (i === j) continue
        pushCandidate(`${combineSource[i]}${combineSource[j]}`)
        if (result.length >= MAX_GENERATED_CANDIDATES) {
          return result
        }
      }
    }
  }

  return result
}
