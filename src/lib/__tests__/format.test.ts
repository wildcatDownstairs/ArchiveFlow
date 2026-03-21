/**
 * @fileoverview 文件功能：提供 format.test 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies 无
 */

import { formatFileSize, formatDateTime, getFileNameFromPath, formatElapsed, formatSpeed } from "../format"

describe("formatFileSize", () => {
  it("returns '0 B' for 0 bytes", () => {
    expect(formatFileSize(0)).toBe("0 B")
  })

  it("returns bytes without decimals for < 1 KB", () => {
    expect(formatFileSize(1023)).toBe("1023 B")
  })

  it("returns '1.0 KB' for exactly 1024 bytes", () => {
    expect(formatFileSize(1024)).toBe("1.0 KB")
  })

  it("returns '1.0 MB' for exactly 1048576 bytes", () => {
    expect(formatFileSize(1048576)).toBe("1.0 MB")
  })

  it("returns '1.0 GB' for exactly 1073741824 bytes", () => {
    expect(formatFileSize(1073741824)).toBe("1.0 GB")
  })

  it("returns '1.5 KB' for 1536 bytes", () => {
    expect(formatFileSize(1536)).toBe("1.5 KB")
  })
})

describe("formatDateTime", () => {
  it("returns a non-empty string for a valid ISO date", () => {
    const result = formatDateTime("2024-06-15T10:30:00Z")
    expect(result).toBeTruthy()
    expect(result.length).toBeGreaterThan(0)
  })

  it("contains the year from the input date", () => {
    const result = formatDateTime("2024-06-15T10:30:00Z")
    expect(result).toContain("2024")
  })

  it("returns a string for another known date", () => {
    const result = formatDateTime("2023-01-01T00:00:00Z")
    expect(result).toContain("2023")
  })
})

describe("getFileNameFromPath", () => {
  it("extracts filename from Windows path", () => {
    expect(getFileNameFromPath("C:\\Users\\test\\file.zip")).toBe("file.zip")
  })

  it("extracts filename from Unix path", () => {
    expect(getFileNameFromPath("/home/user/file.zip")).toBe("file.zip")
  })

  it("returns filename if no directory separator", () => {
    expect(getFileNameFromPath("file.zip")).toBe("file.zip")
  })

  it("returns empty string for empty input", () => {
    expect(getFileNameFromPath("")).toBe("")
  })
})

describe("formatElapsed", () => {
  it("formats 30 seconds as '30.0s'", () => {
    expect(formatElapsed(30)).toBe("30.0s")
  })

  it("formats 0 seconds as '0.0s'", () => {
    expect(formatElapsed(0)).toBe("0.0s")
  })

  it("formats 59.9 seconds as '59.9s'", () => {
    expect(formatElapsed(59.9)).toBe("59.9s")
  })

  it("formats 60 seconds as '1m 0s'", () => {
    expect(formatElapsed(60)).toBe("1m 0s")
  })

  it("formats 90 seconds as '1m 30s'", () => {
    expect(formatElapsed(90)).toBe("1m 30s")
  })

  it("formats 3661 seconds as '61m 1s'", () => {
    expect(formatElapsed(3661)).toBe("61m 1s")
  })
})

describe("formatSpeed", () => {
  it("formats 0 as '0'", () => {
    expect(formatSpeed(0)).toBe("0")
  })

  it("formats 500 as '500'", () => {
    expect(formatSpeed(500)).toBe("500")
  })

  it("formats 1500 with thousands separator", () => {
    expect(formatSpeed(1500)).toBe("1,500")
  })

  it("formats 999999 with thousands separator", () => {
    expect(formatSpeed(999999)).toBe("999,999")
  })

  it("formats 1000000 as '1.0M'", () => {
    expect(formatSpeed(1000000)).toBe("1.0M")
  })

  it("formats 118845703 as '118.8M'", () => {
    expect(formatSpeed(118845703)).toBe("118.8M")
  })

  it("formats 2300000000 as '2.30G'", () => {
    expect(formatSpeed(2300000000)).toBe("2.30G")
  })
})
