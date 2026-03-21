/**
 * @fileoverview 文件功能：提供 fileTree 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies 无
 */

import type { ArchiveEntry } from "@/types"

export interface TreeNode {
  name: string
  path: string
  entry?: ArchiveEntry
  children: TreeNode[]
  isDirectory: boolean
}

/**
 *
 * @param entries
  * @returns {any} 执行结果
 */
export function buildFileTree(entries: ArchiveEntry[]): TreeNode[] {
  const root: TreeNode[] = []

  for (const entry of entries) {
    const parts = entry.path.split("/").filter(Boolean)
    let currentLevel = root

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]
      const isLast = i === parts.length - 1
      const existing = currentLevel.find((n) => n.name === part)

      if (existing) {
        if (isLast && !entry.is_directory) {
          existing.entry = entry
        }
        currentLevel = existing.children
      } else {
        const node: TreeNode = {
          name: part,
          path: parts.slice(0, i + 1).join("/"),
          entry: isLast ? entry : undefined,
          children: [],
          isDirectory: isLast ? entry.is_directory : true,
        }
        currentLevel.push(node)
        currentLevel = node.children
      }
    }
  }

  // 排序：目录在前，文件在后，同类按名称
  /**
   *
   * @param nodes
   */
  const sortNodes = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
      return a.name.localeCompare(b.name)
    })
    for (const node of nodes) {
      sortNodes(node.children)
    }
  }
  sortNodes(root)

  return root
}
