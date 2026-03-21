import { create } from "zustand"

const LOCALE_STORAGE_KEY = "archiveflow.locale"
const RECOVERY_PREFERENCES_STORAGE_KEY = "archiveflow.recovery-preferences"
const RECOVERY_DRAFTS_STORAGE_KEY = "archiveflow.recovery-drafts"

export interface CharsetFlags {
  lowercase: boolean
  uppercase: boolean
  digits: boolean
  special: boolean
}

export type ResultRetentionPolicy = "plaintext" | "masked"

export interface RecoveryPreferences {
  defaultCharsetFlags: CharsetFlags
  defaultMinLength: number
  defaultMaxLength: number
  defaultTaskPriority: number
  autoIncludeFilenamePatterns: boolean
  autoClearDictionaryInput: boolean
  resultRetentionPolicy: ResultRetentionPolicy
  exportMaskPasswords: boolean
  exportIncludeAuditEvents: boolean
  maxConcurrentRecoveries: number
}

export interface RecoveryDrafts {
  dictionaryText: string
  dictionarySourceName: string | null
}

const DEFAULT_RECOVERY_PREFERENCES: RecoveryPreferences = {
  defaultCharsetFlags: {
    lowercase: true,
    uppercase: false,
    digits: true,
    special: false,
  },
  defaultMinLength: 1,
  defaultMaxLength: 4,
  defaultTaskPriority: 0,
  autoIncludeFilenamePatterns: false,
  autoClearDictionaryInput: false,
  resultRetentionPolicy: "plaintext",
  exportMaskPasswords: false,
  exportIncludeAuditEvents: true,
  maxConcurrentRecoveries: 1,
}

const DEFAULT_RECOVERY_DRAFTS: RecoveryDrafts = {
  dictionaryText: "",
  dictionarySourceName: null,
}

function getInitialLocale(): string {
  if (typeof window === "undefined") return "zh"

  try {
    const storedLocale = window.localStorage.getItem(LOCALE_STORAGE_KEY)
    return storedLocale === "en" ? "en" : "zh"
  } catch {
    return "zh"
  }
}

function getInitialRecoveryPreferences(): RecoveryPreferences {
  if (typeof window === "undefined") return DEFAULT_RECOVERY_PREFERENCES

  try {
    const stored = window.localStorage.getItem(RECOVERY_PREFERENCES_STORAGE_KEY)
    if (!stored) return DEFAULT_RECOVERY_PREFERENCES

    const parsed = JSON.parse(stored) as Partial<RecoveryPreferences>
    return {
      ...DEFAULT_RECOVERY_PREFERENCES,
      ...parsed,
      defaultCharsetFlags: {
        ...DEFAULT_RECOVERY_PREFERENCES.defaultCharsetFlags,
        ...parsed.defaultCharsetFlags,
      },
    }
  } catch {
    return DEFAULT_RECOVERY_PREFERENCES
  }
}

function getInitialRecoveryDrafts(): RecoveryDrafts {
  if (typeof window === "undefined") return DEFAULT_RECOVERY_DRAFTS

  try {
    const stored = window.localStorage.getItem(RECOVERY_DRAFTS_STORAGE_KEY)
    if (!stored) return DEFAULT_RECOVERY_DRAFTS

    const parsed = JSON.parse(stored) as Partial<RecoveryDrafts>
    return {
      ...DEFAULT_RECOVERY_DRAFTS,
      ...parsed,
    }
  } catch {
    return DEFAULT_RECOVERY_DRAFTS
  }
}

interface AppState {
  locale: string
  setLocale: (locale: string) => void
  recoveryPreferences: RecoveryPreferences
  updateRecoveryPreferences: (patch: Partial<RecoveryPreferences>) => void
  recoveryDrafts: RecoveryDrafts
  updateRecoveryDrafts: (patch: Partial<RecoveryDrafts>) => void
  sidebarCollapsed: boolean
  toggleSidebar: () => void
}

export const useAppStore = create<AppState>((set) => ({
  locale: getInitialLocale(),
  setLocale: (locale: string) => {
    if (typeof window !== "undefined") {
      try {
        window.localStorage.setItem(LOCALE_STORAGE_KEY, locale)
      } catch {
        // Ignore storage failures and keep in-memory state consistent.
      }
    }
    set({ locale })
  },
  recoveryPreferences: getInitialRecoveryPreferences(),
  updateRecoveryPreferences: (patch) =>
    set((state) => {
      const next = {
        ...state.recoveryPreferences,
        ...patch,
        defaultCharsetFlags: {
          ...state.recoveryPreferences.defaultCharsetFlags,
          ...patch.defaultCharsetFlags,
        },
      }

      if (typeof window !== "undefined") {
        try {
          window.localStorage.setItem(
            RECOVERY_PREFERENCES_STORAGE_KEY,
            JSON.stringify(next),
          )
        } catch {
          // Ignore storage failures and keep in-memory state consistent.
        }
      }

      return { recoveryPreferences: next }
    }),
  recoveryDrafts: getInitialRecoveryDrafts(),
  updateRecoveryDrafts: (patch) =>
    set((state) => {
      const next = {
        ...state.recoveryDrafts,
        ...patch,
      }

      if (typeof window !== "undefined") {
        try {
          window.localStorage.setItem(
            RECOVERY_DRAFTS_STORAGE_KEY,
            JSON.stringify(next),
          )
        } catch {
          // Ignore storage failures and keep in-memory state consistent.
        }
      }

      return { recoveryDrafts: next }
    }),
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
}))
