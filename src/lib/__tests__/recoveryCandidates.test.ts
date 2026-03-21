/**
 * @fileoverview 文件功能：提供 recoveryCandidates.test 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies vitest
 */

import { describe, expect, it } from "vitest"
import { buildDictionaryCandidates } from "@/lib/recoveryCandidates"

describe("buildDictionaryCandidates", () => {
  it("generates reverse, duplicate and year-based variants", () => {
    const candidates = buildDictionaryCandidates(
      ["secret"],
      "archive.zip",
      {
        uppercase: false,
        capitalize: false,
        leetspeak: false,
        reverse: true,
        duplicate: true,
        yearPatterns: true,
        separatorPatterns: false,
        commonSuffixes: false,
        combineWords: false,
        includeFilenamePatterns: false,
      },
    )

    expect(candidates).toContain("secret")
    expect(candidates).toContain("terces")
    expect(candidates).toContain("secretsecret")
    expect(candidates).toContain("secret2026")
    expect(candidates).toContain("Secret26")
  })

  it("generates separator-based combinations without duplicates", () => {
    const candidates = buildDictionaryCandidates(
      ["alpha", "beta"],
      "archive.zip",
      {
        uppercase: false,
        capitalize: false,
        leetspeak: false,
        reverse: false,
        duplicate: false,
        yearPatterns: false,
        separatorPatterns: true,
        commonSuffixes: false,
        combineWords: true,
        includeFilenamePatterns: false,
      },
    )

    expect(candidates).toContain("alpha-beta")
    expect(candidates).toContain("alpha_beta")
    expect(candidates).toContain("alpha.beta")
    expect(candidates).toContain("alphabeta")
    expect(new Set(candidates).size).toBe(candidates.length)
  })

  it("can add filename seeds and common suffixes for task-specific candidates", () => {
    const candidates = buildDictionaryCandidates(
      ["secret"],
      "project_launch-plan.zip",
      {
        uppercase: false,
        capitalize: false,
        leetspeak: false,
        reverse: false,
        duplicate: false,
        yearPatterns: false,
        separatorPatterns: false,
        commonSuffixes: true,
        combineWords: false,
        includeFilenamePatterns: true,
      },
    )

    expect(candidates).toContain("secret123")
    expect(candidates).toContain("project")
    expect(candidates).toContain("launch")
    expect(candidates).toContain("plan")
  })
})
