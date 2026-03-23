/**
 * @fileoverview 文件功能：提供 format 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies 无
 */

/**
 *
 * @param bytes
  * @returns {any} 执行结果
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B"
  const units = ["B", "KB", "MB", "GB", "TB"]
  // 魔法数字：1024 作为单位换算的基数
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`
}

/**
 *
 * @param isoString
  * @returns {any} 执行结果
 */
export function formatDateTime(isoString: string): string {
  const date = new Date(isoString)
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  })
}

/**
 *
 * @param filePath
  * @returns {any} 执行结果
 */
export function getFileNameFromPath(filePath: string): string {
  return filePath.split(/[\\/]/).pop() || filePath
}

/**
 *
 * @param seconds
  * @returns {any} 执行结果
 */
export function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(1)}s`
  const min = Math.floor(seconds / 60)
  const sec = seconds % 60
  return `${min}m ${sec.toFixed(0)}s`
}

/**
 * 将大数字格式化为中文量级的粗略表示，末尾加"余"。
 * 例如: 663_420_431_289_062 → "663兆4200亿余"
 *       6_634_204_312 → "66亿3420万余"
 *       800_237_568 → "8亿余"
 *       3_450_000 → "345万余"
 *       12_000 → "1万2000余"
 *       999 → "999"（小数字直接返回原始逗号格式）
 *
 * 规则：取最高两个非零量级显示，不足两级时只显示一级。
 */
export function formatCompactChinese(n: number): string {
  if (n < 10_000) return n.toLocaleString()

  const 兆 = 1_000_000_000_000
  const 亿 = 100_000_000
  const 万 = 10_000

  if (n >= 兆) {
    const zhaoVal = Math.floor(n / 兆)
    const remainder = n % 兆
    const yiVal = Math.floor(remainder / 亿)
    return yiVal > 0 ? `${zhaoVal}兆${yiVal}00亿余` : `${zhaoVal}兆余`
  }

  if (n >= 亿) {
    const yiVal = Math.floor(n / 亿)
    const remainder = n % 亿
    const wanVal = Math.floor(remainder / 万)
    return wanVal > 0 ? `${yiVal}亿${wanVal}万余` : `${yiVal}亿余`
  }

  // 万级
  const wanVal = Math.floor(n / 万)
  const remainder = n % 万
  return remainder > 0 ? `${wanVal}万${remainder}余` : `${wanVal}万余`
}

/**
 * 将每秒密码尝试数格式化为易读的缩写形式。
 * 例如: 118845703 → "118.8M", 1500 → "1,500", 2300000000 → "2.30G"
 */
export function formatSpeed(speed: number): string {
  if (speed >= 1e9) return `${(speed / 1e9).toFixed(2)}G`
  if (speed >= 1e6) return `${(speed / 1e6).toFixed(1)}M`
  if (speed >= 1e3) return speed.toLocaleString("en-US", { maximumFractionDigits: 0 })
  return speed.toFixed(0)
}

/**
 *
 * @param format
 * @param fileName
  * @returns {any} 执行结果
 */
export function buildExportFileName(format: string, fileName?: string): string {
  // 魔法数字与正则：格式化时间戳并移除不需要的字符，截取前 15 位
  const timestamp = new Date()
    .toISOString()
    .replace(/[-:]/g, "")
    .replace("T", "-")
    .slice(0, 15)

  if (fileName) {
    const sanitizedName = fileName
      .replace(/\.[^.]+$/, "")
      .replace(/[\\/:*?"<>|]/g, "-")
      .trim()
      .slice(0, 60) || "task"
    return `archiveflow-${sanitizedName}-${timestamp}.${format}`
  }
  return `archiveflow-export-all-${timestamp}.${format}`
}
