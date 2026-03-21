function buildYearValues(): string[] {
  const currentYear = new Date().getFullYear()
  const years: string[] = []
  for (let offset = 0; offset <= 2; offset++) {
    const year = currentYear - offset
    years.push(String(year), String(year).slice(-2))
  }
  return years
}

const YEAR_PATTERNS = buildYearValues()
const COMMON_SUFFIXES = ["1", "12", "123", "1234", ...YEAR_PATTERNS.filter((y) => y.length === 4), "!", "@123"]
const COMBINATION_SEPARATORS = ["", "-", "_", "."]
const MAX_GENERATED_CANDIDATES = 50_000

export interface DictionaryGenerationOptions {
  uppercase: boolean
  capitalize: boolean
  leetspeak: boolean
  reverse: boolean
  duplicate: boolean
  yearPatterns: boolean
  separatorPatterns: boolean
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

function reverseWord(word: string): string {
  return Array.from(word).reverse().join("")
}

function filenameSeeds(fileName: string): string[] {
  const stem = fileName.replace(/\.[^.]+$/, "")
  return stem
    .split(/[^a-zA-Z0-9]+/)
    .map((part) => part.trim())
    .filter((part) => part.length >= 2)
}

function pushYearPatterns(
  pushCandidate: (candidate: string) => void,
  seed: string,
) {
  for (const year of YEAR_PATTERNS) {
    pushCandidate(`${seed}${year}`)
    pushCandidate(`${capitalizeWord(seed)}${year}`)
  }
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
    if (options.reverse) pushCandidate(reverseWord(seed))
    if (options.duplicate) pushCandidate(`${seed}${seed}`)
    if (options.yearPatterns) pushYearPatterns(pushCandidate, seed)
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
        if (options.separatorPatterns) {
          for (const separator of COMBINATION_SEPARATORS) {
            pushCandidate(`${combineSource[i]}${separator}${combineSource[j]}`)
          }
        } else {
          pushCandidate(`${combineSource[i]}${combineSource[j]}`)
        }
        if (result.length >= MAX_GENERATED_CANDIDATES) {
          return result
        }
      }
    }
  }

  return result
}
