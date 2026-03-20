import { create } from "zustand"

const LOCALE_STORAGE_KEY = "archiveflow.locale"

function getInitialLocale(): string {
  if (typeof window === "undefined") return "zh"

  try {
    const storedLocale = window.localStorage.getItem(LOCALE_STORAGE_KEY)
    return storedLocale === "en" ? "en" : "zh"
  } catch {
    return "zh"
  }
}

interface AppState {
  locale: string
  setLocale: (locale: string) => void
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
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
}))
