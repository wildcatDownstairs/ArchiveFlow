import { create } from "zustand"

interface AppState {
  locale: string
  setLocale: (locale: string) => void
  sidebarCollapsed: boolean
  toggleSidebar: () => void
}

export const useAppStore = create<AppState>((set) => ({
  locale: "zh",
  setLocale: (locale: string) => set({ locale }),
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
}))
