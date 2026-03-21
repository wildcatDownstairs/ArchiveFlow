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
})
