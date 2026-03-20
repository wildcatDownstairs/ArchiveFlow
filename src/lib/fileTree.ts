import type { ArchiveEntry } from "@/types"

export interface TreeNode {
  name: string
  path: string
  entry?: ArchiveEntry
  children: TreeNode[]
  isDirectory: boolean
}

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
