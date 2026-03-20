import { buildFileTree } from "../fileTree"
import type { ArchiveEntry } from "@/types"

function makeEntry(overrides: Partial<ArchiveEntry> & Pick<ArchiveEntry, "path">): ArchiveEntry {
  return {
    size: 0,
    compressed_size: 0,
    is_directory: false,
    is_encrypted: false,
    last_modified: null,
    ...overrides,
  }
}

describe("buildFileTree", () => {
  it("returns empty array for empty entries", () => {
    expect(buildFileTree([])).toEqual([])
  })

  it("creates a single file node for a root-level file", () => {
    const entries = [makeEntry({ path: "README.md" })]
    const tree = buildFileTree(entries)

    expect(tree).toHaveLength(1)
    expect(tree[0].name).toBe("README.md")
    expect(tree[0].isDirectory).toBe(false)
    expect(tree[0].children).toEqual([])
  })

  it("creates a single directory node", () => {
    const entries = [makeEntry({ path: "src/", is_directory: true })]
    const tree = buildFileTree(entries)

    expect(tree).toHaveLength(1)
    expect(tree[0].name).toBe("src")
    expect(tree[0].isDirectory).toBe(true)
  })

  it("nests a file inside a directory for 'src/main.ts'", () => {
    const entries = [makeEntry({ path: "src/main.ts" })]
    const tree = buildFileTree(entries)

    expect(tree).toHaveLength(1)
    expect(tree[0].name).toBe("src")
    expect(tree[0].isDirectory).toBe(true)
    expect(tree[0].children).toHaveLength(1)
    expect(tree[0].children[0].name).toBe("main.ts")
    expect(tree[0].children[0].isDirectory).toBe(false)
  })

  it("builds a complex tree with correct nesting and sorting", () => {
    const entries = [
      makeEntry({ path: "src/utils/helper.ts" }),
      makeEntry({ path: "src/main.ts" }),
      makeEntry({ path: "README.md" }),
      makeEntry({ path: "src/components/", is_directory: true }),
      makeEntry({ path: "src/components/Button.tsx" }),
    ]
    const tree = buildFileTree(entries)

    // Root level: directories before files → "src" then "README.md"
    expect(tree).toHaveLength(2)
    expect(tree[0].name).toBe("src")
    expect(tree[0].isDirectory).toBe(true)
    expect(tree[1].name).toBe("README.md")
    expect(tree[1].isDirectory).toBe(false)

    // src children: directories before files → "components", "utils", then "main.ts"
    const srcChildren = tree[0].children
    expect(srcChildren).toHaveLength(3)
    expect(srcChildren[0].name).toBe("components")
    expect(srcChildren[0].isDirectory).toBe(true)
    expect(srcChildren[1].name).toBe("utils")
    expect(srcChildren[1].isDirectory).toBe(true)
    expect(srcChildren[2].name).toBe("main.ts")
    expect(srcChildren[2].isDirectory).toBe(false)

    // components has Button.tsx
    expect(srcChildren[0].children).toHaveLength(1)
    expect(srcChildren[0].children[0].name).toBe("Button.tsx")
  })

  it("merges entries with the same directory prefix", () => {
    const entries = [
      makeEntry({ path: "lib/a.ts" }),
      makeEntry({ path: "lib/b.ts" }),
      makeEntry({ path: "lib/c.ts" }),
    ]
    const tree = buildFileTree(entries)

    // Only one "lib" directory node
    expect(tree).toHaveLength(1)
    expect(tree[0].name).toBe("lib")
    expect(tree[0].isDirectory).toBe(true)
    expect(tree[0].children).toHaveLength(3)
    expect(tree[0].children.map((n) => n.name)).toEqual(["a.ts", "b.ts", "c.ts"])
  })

  it("attaches entry data to file nodes", () => {
    const entries = [makeEntry({ path: "data.json", size: 1234, is_encrypted: true })]
    const tree = buildFileTree(entries)

    expect(tree[0].entry).toBeDefined()
    expect(tree[0].entry!.size).toBe(1234)
    expect(tree[0].entry!.is_encrypted).toBe(true)
  })
})
