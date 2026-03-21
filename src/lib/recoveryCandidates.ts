/**
 * @fileoverview 文件功能：提供 recoveryCandidates 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies 无
 */

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
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
// 魔法数字：定义常见后缀，用于增强字典攻击的覆盖率
const COMMON_SUFFIXES = ["1", "12", "123", "1234", ...YEAR_PATTERNS.filter((y) => y.length === 4), "!", "@123"]
const COMBINATION_SEPARATORS = ["", "-", "_", "."]
// 魔法数字：最大生成的候选密码数量，防止内存溢出
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

/**
 *
 * @param word
  * @returns {any} 执行结果
 */
function capitalizeWord(word: string): string {
  if (word.length === 0) return word
  return word[0].toUpperCase() + word.slice(1).toLowerCase()
}

/**
 *
 * @param word
  * @returns {any} 执行结果
 */
function leetspeakVariant(word: string): string {
  // 复杂正则：使用正则表达式进行 Leet 变体替换
  return word
    .replace(/a/gi, "@")
    .replace(/e/gi, "3")
    .replace(/i/gi, "1")
    .replace(/o/gi, "0")
    .replace(/s/gi, "$")
}

/**
 *
 * @param word
  * @returns {any} 执行结果
 */
function reverseWord(word: string): string {
  return Array.from(word).reverse().join("")
}

/**
 *
 * @param fileName
  * @returns {any} 执行结果
 */
function filenameSeeds(fileName: string): string[] {
  // 复杂业务逻辑：移除文件扩展名
  const stem = fileName.replace(/\.[^.]+$/, "")
  // 使用正则提取所有可能的字母数字片段
  return stem
    .split(/[^a-zA-Z0-9]+/)
    .map((part) => part.trim())
    .filter((part) => part.length >= 2)
}

/**
 *
 * @param pushCandidate
 * @param seed
  * @returns {any} 执行结果
 */
function pushYearPatterns(
  pushCandidate: (candidate: string) => void,
  seed: string,
) {
  for (const year of YEAR_PATTERNS) {
    pushCandidate(`${seed}${year}`)
    pushCandidate(`${capitalizeWord(seed)}${year}`)
  }
}

/**
 *
 * @param baseWords
 * @param fileName
 * @param options
  * @returns {any} 执行结果
 */
export function buildDictionaryCandidates(
  baseWords: string[],
  fileName: string,
  options: DictionaryGenerationOptions,
): string[] {
  const seen = new Set<string>()
  const result: string[] = []

  /**
   *
   * @param candidate
   */
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
