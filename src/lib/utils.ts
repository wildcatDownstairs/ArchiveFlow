/**
 * @fileoverview 文件功能：提供 utils 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies clsx, tailwind-merge
 */

import { type ClassValue, clsx } from "clsx"
import { twMerge } from "tailwind-merge"

/**
 *
 * @param {...any} inputs
  * @returns {any} 执行结果
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
