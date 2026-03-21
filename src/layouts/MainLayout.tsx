import { useEffect, useState } from "react"
import { flushSync } from "react-dom"
import { NavLink, Outlet } from "react-router-dom"
import { useTranslation } from "react-i18next"
import {
  Home,
  ListTodo,
  FileText,
  Settings,
} from "lucide-react"
import { cn } from "@/lib/utils"

const navItems = [
  { to: "/", icon: Home, labelKey: "home" },
  { to: "/tasks", icon: ListTodo, labelKey: "tasks" },
  { to: "/reports", icon: FileText, labelKey: "reports" },
  { to: "/settings", icon: Settings, labelKey: "settings" },
] as const

const THEME_STORAGE_KEY = "archiveflow.theme-mode"

type ThemeMode = "dark" | "light"
type ThemeTransitionDocument = Document & {
  startViewTransition?: (callback: () => void) => {
    finished: Promise<void>
  }
}

function getInitialThemeMode(): ThemeMode {
  if (typeof window === "undefined") return "dark"

  try {
    const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY)
    const nextTheme = storedTheme === "light" ? "light" : "dark"
    document.documentElement.dataset.theme = nextTheme
    return nextTheme
  } catch {
    document.documentElement.dataset.theme = "dark"
    return "dark"
  }
}

export default function MainLayout() {
  const { t } = useTranslation()
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => getInitialThemeMode())

  useEffect(() => {
    document.documentElement.dataset.theme = themeMode

    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, themeMode)
    } catch {
      // Ignore storage failures and keep the in-memory theme state.
    }
  }, [themeMode])

  const isLightTheme = themeMode === "light"
  const toggleTheme = () => {
    const nextTheme: ThemeMode = isLightTheme ? "dark" : "light"
    const prefersReducedMotion =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches

    const transitionDocument = document as ThemeTransitionDocument
    if (!prefersReducedMotion && transitionDocument.startViewTransition) {
      transitionDocument.startViewTransition(() => {
        flushSync(() => setThemeMode(nextTheme))
      })
      return
    }

    setThemeMode(nextTheme)
  }

  return (
    <div className="flex h-screen min-h-screen flex-col overflow-hidden lg:flex-row">
      <aside className="af-sidebar-shell shrink-0 border-b backdrop-blur lg:sticky lg:top-0 lg:h-screen lg:w-[224px] lg:flex-shrink-0 lg:border-b-0 lg:border-r">
        <div className="flex h-full flex-col px-4 py-6">
          <div className="flex items-center justify-between lg:block">
            <div className="af-sidebar-brand px-2 text-[18px] font-bold text-foreground">
              ArchiveFlow
            </div>

            {/* 小屏时显示在右上角，大屏时隐藏（大屏按钮在底部） */}
            <button
              type="button"
              aria-pressed={themeMode === "dark"}
              aria-label={isLightTheme ? t("theme_switch_to_dark") : t("theme_switch_to_light")}
              onClick={toggleTheme}
              className="af-theme-switch lg:hidden"
            >
              <span className="af-theme-switch__content">
                <svg aria-hidden="true" className="af-theme-switch__backdrop" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 290 120"><g><path fill="rgba(200,190,178,0.65)" d="M295 72c0 13.8-11.2 25-25 25a25 25 0 0 1-4.6-.42c-1.2 6.24-5.24 11.47-10.7 14.35C248.5 118 240 125 229 125a25 25 0 0 1-7.93-1.28A20.9 20.9 0 0 1 209 127a20.9 20.9 0 0 1-11.33-3.35A21 21 0 0 1 192 125a20.9 20.9 0 0 1-9.46-2.27A20.9 20.9 0 0 1 168 127c-.9 0-1.79-.06-2.67-.17C161.5 133.7 155.8 137 149 137c-11.5 0-20.8-9.3-20.8-20.8s9.3-20.8 20.8-20.8c.9 0 1.79.06 2.67.17C155.5 88.3 161.2 85 168 85a20.9 20.9 0 0 1 9.46 2.27C184.26 83.37 189.1 81 194 81a20.9 20.9 0 0 1 11.33 3.35 20.9 20.9 0 0 1 3.58-.38C215.26 79.38 223.1 74.5 232 74.5c.3 0 .59 0 .89.02A20.8 20.8 0 0 1 245 71h.001C245.13 57.3 256.5 47 268.5 47c13.8 0 26.5 11.2 26.5 25Z"/></g></svg>
                <svg aria-hidden="true" className="af-theme-switch__backdrop" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 290 120"><g className="af-theme-switch__stars"><g><path fill="#fff" fillRule="evenodd" d="M52 8a.6.6 0 0 1 .578.435l.65 2.276a3 3 0 0 0 2.061 2.061l2.276.65a.6.6 0 0 1 0 1.156l-2.276.65a3 3 0 0 0-2.061 2.061l-.65 2.276a.6.6 0 0 1-1.156 0l-.65-2.276a3 3 0 0 0-2.061-2.061l-2.276-.65a.6.6 0 0 1 0-1.156l2.276-.65A3 3 0 0 0 50.772 8.435L51.422 6.16A.6.6 0 0 1 52 8Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M53.5 36a.263.263 0 0 1 .252.19l.285.997a1.313 1.313 0 0 0 .902.901l.997.285a.263.263 0 0 1 0 .504l-.997.285a1.313 1.313 0 0 0-.902.901l-.285.997a.263.263 0 0 1-.504 0l-.285-.997a1.313 1.313 0 0 0-.902-.901l-.997-.285a.263.263 0 0 1 0-.504l.997-.285a1.313 1.313 0 0 0 .902-.901l.285-.997A.263.263 0 0 1 53.5 36Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M28 26a.224.224 0 0 1 .216.163l.244.854a1.123 1.123 0 0 0 .773.773l.854.244a.224.224 0 0 1 0 .432l-.854.244a1.123 1.123 0 0 0-.773.773l-.244.854a.224.224 0 0 1-.432 0l-.244-.854a1.123 1.123 0 0 0-.773-.773l-.854-.244a.224.224 0 0 1 0-.432l.854-.244a1.123 1.123 0 0 0 .773-.773l.244-.854A.225.225 0 0 1 28 26Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M36 62a.263.263 0 0 1 .252.19l.285.997a1.313 1.313 0 0 0 .902.901l.997.285a.263.263 0 0 1 0 .504l-.997.285a1.313 1.313 0 0 0-.902.901l-.285.997a.263.263 0 0 1-.504 0l-.285-.997a1.313 1.313 0 0 0-.902-.901l-.997-.285a.263.263 0 0 1 0-.504l.997-.285a1.313 1.313 0 0 0 .902-.901l.285-.997A.263.263 0 0 1 36 62Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M124 22a.6.6 0 0 1 .578.435l.65 2.276a3 3 0 0 0 2.061 2.061l2.276.65a.6.6 0 0 1 0 1.156l-2.276.65a3 3 0 0 0-2.061 2.061l-.65 2.276a.6.6 0 0 1-1.156 0l-.65-2.276a3 3 0 0 0-2.061-2.061l-2.276-.65a.6.6 0 0 1 0-1.156l2.276-.65a3 3 0 0 0 2.061-2.061l.65-2.276A.6.6 0 0 1 124 22Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M100 52a.3.3 0 0 1 .288.217l.325 1.139a1.5 1.5 0 0 0 1.031 1.031l1.139.325a.3.3 0 0 1 0 .576l-1.139.325a1.5 1.5 0 0 0-1.031 1.031l-.325 1.139a.3.3 0 0 1-.576 0l-.325-1.139a1.5 1.5 0 0 0-1.031-1.031l-1.139-.325a.3.3 0 0 1 0-.576l1.139-.325A1.5 1.5 0 0 0 99.387 53l.325-1.139A.3.3 0 0 1 100 52Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M115 68a.41.41 0 0 1 .397.299l.447 1.565a2.057 2.057 0 0 0 1.416 1.416l1.565.447a.411.411 0 0 1 0 .792l-1.565.447a2.057 2.057 0 0 0-1.416 1.416l-.447 1.565a.411.411 0 0 1-.794 0l-.447-1.565a2.057 2.057 0 0 0-1.416-1.416l-1.565-.447a.411.411 0 0 1 0-.792l1.565-.447a2.057 2.057 0 0 0 1.416-1.416l.447-1.565A.41.41 0 0 1 115 68Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M165 16a.263.263 0 0 1 .252.19l.285.997a1.313 1.313 0 0 0 .902.901l.997.285a.263.263 0 0 1 0 .504l-.997.285a1.313 1.313 0 0 0-.902.901l-.285.997a.263.263 0 0 1-.504 0l-.285-.997a1.313 1.313 0 0 0-.902-.901l-.997-.285a.263.263 0 0 1 0-.504l.997-.285a1.313 1.313 0 0 0 .902-.901l.285-.997A.263.263 0 0 1 165 16Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M188 8a.6.6 0 0 1 .578.435l.65 2.276a3 3 0 0 0 2.061 2.061l2.276.65a.6.6 0 0 1 0 1.156l-2.276.65a3 3 0 0 0-2.061 2.061l-.65 2.276a.6.6 0 0 1-1.156 0l-.65-2.276a3 3 0 0 0-2.061-2.061l-2.276-.65a.6.6 0 0 1 0-1.156l2.276-.65a3 3 0 0 0 2.061-2.061l.65-2.276A.6.6 0 0 1 188 8Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M178 36a.3.3 0 0 1 .288.217l.325 1.139a1.5 1.5 0 0 0 1.031 1.031l1.139.325a.3.3 0 0 1 0 .576l-1.139.325a1.5 1.5 0 0 0-1.031 1.031l-.325 1.139a.3.3 0 0 1-.576 0l-.325-1.139a1.5 1.5 0 0 0-1.031-1.031l-1.139-.325a.3.3 0 0 1 0-.576l1.139-.325A1.5 1.5 0 0 0 178.613 37l.325-1.139A.3.3 0 0 1 178 36Z" clipRule="evenodd" /></g><g><path fill="#fff" fillRule="evenodd" d="M155 52a.41.41 0 0 1 .397.299l.447 1.565a2.057 2.057 0 0 0 1.416 1.416l1.565.447a.411.411 0 0 1 0 .792l-1.565.447a2.057 2.057 0 0 0-1.416 1.416l-.447 1.565a.411.411 0 0 1-.794 0l-.447-1.565a2.057 2.057 0 0 0-1.416-1.416l-1.565-.447a.411.411 0 0 1 0-.792l1.565-.447a2.057 2.057 0 0 0 1.416-1.416l.447-1.565A.41.41 0 0 1 155 52Z" clipRule="evenodd" /></g></g></svg>
                <svg aria-hidden="true" className="af-theme-switch__backdrop" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 290 120"><g><path fill="rgba(255,255,255,0.97)" d="M290 88c0 11.6-6.1 21.8-15.3 27.5l.008.71c0 17.9-14.5 32.4-32.4 32.4a32.6 32.6 0 0 1-8.36-1.09C228.3 155.6 215.8 166 201 166a32.6 32.6 0 0 1-5.88-.532C189.2 170.4 180.8 174 171.5 174a32.3 32.3 0 0 1-21.26-7.95A32.3 32.3 0 0 1 139.5 168a32.3 32.3 0 0 1-9.56-1.43C123.3 172.6 114.4 176 104.5 176a32.3 32.3 0 0 1-12.06-2.32A32.3 32.3 0 0 1 70.5 178c-17.9 0-32.4-14.5-32.4-32.4s14.5-32.4 32.4-32.4a32.3 32.3 0 0 1 12.06 2.32A32.3 32.3 0 0 1 104.5 111c3.33 0 6.54.5 9.56 1.43C120.7 106.4 129.6 103 139.5 103a32.3 32.3 0 0 1 21.26 7.95A32.3 32.3 0 0 1 171.5 109c2.01 0 3.98.182 5.88.532C183.3 104.6 191.8 101 201 101c2.89 0 5.69.379 8.36 1.09 2.16-8.01 7.3-14.79 14.16-19.08l-.008-.71C223.508 65 237.5 53 253.5 53S290 67.5 290 88Z"/></g></svg>
                <span className="af-theme-switch__indicator-wrap">
                  <span className="af-theme-switch__indicator">
                    <span className="af-theme-switch__sphere">
                      <span className="af-theme-switch__sun">
                        <span className="af-theme-switch__moon">
                          <span className="af-theme-switch__crater" />
                          <span className="af-theme-switch__crater" />
                          <span className="af-theme-switch__crater" />
                        </span>
                      </span>
                    </span>
                  </span>
                </span>
              </span>
            </button>
          </div>

          <nav className="af-scrollbar-none mt-6 flex gap-2 overflow-x-auto lg:flex-1 lg:flex-col lg:overflow-visible">
            {navItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.to === "/"}
                className={({ isActive }) =>
                  cn(
                    "af-sidebar-link text-[14px] font-medium",
                    isActive && "af-sidebar-link-active",
                  )
                }
              >
                <item.icon className="h-4 w-4 flex-shrink-0" />
                <span className="whitespace-nowrap">{t(item.labelKey)}</span>
              </NavLink>
            ))}
          </nav>

          <div className="mt-8 hidden border-t border-white/6 px-1 pt-6 pb-3 lg:mt-auto lg:block">
            <div className="flex items-center justify-center">
              <button
                type="button"
                aria-pressed={themeMode === "dark"}
                aria-label={isLightTheme ? t("theme_switch_to_dark") : t("theme_switch_to_light")}
                onClick={toggleTheme}
                className="af-theme-switch"
              >
                <span className="af-theme-switch__content">
                  <svg
                    aria-hidden="true"
                    className="af-theme-switch__backdrop"
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 290 120"
                  >
                    <g>
                      <path
                        fill="rgba(200,190,178,0.65)"
                        d="M295 72c0 13.8-11.2 25-25 25a25 25 0 0 1-4.6-.42c-1.2 6.24-5.24 11.47-10.7 14.35C248.5 118 240 125 229 125a25 25 0 0 1-7.93-1.28A20.9 20.9 0 0 1 209 127a20.9 20.9 0 0 1-11.33-3.35A21 21 0 0 1 192 125a20.9 20.9 0 0 1-9.46-2.27A20.9 20.9 0 0 1 168 127c-.9 0-1.79-.06-2.67-.17C161.5 133.7 155.8 137 149 137c-11.5 0-20.8-9.3-20.8-20.8s9.3-20.8 20.8-20.8c.9 0 1.79.06 2.67.17C155.5 88.3 161.2 85 168 85a20.9 20.9 0 0 1 9.46 2.27C184.26 83.37 189.1 81 194 81a20.9 20.9 0 0 1 11.33 3.35 20.9 20.9 0 0 1 3.58-.38C215.26 79.38 223.1 74.5 232 74.5c.3 0 .59 0 .89.02A20.8 20.8 0 0 1 245 71h.001C245.13 57.3 256.5 47 268.5 47c13.8 0 26.5 11.2 26.5 25Z"
                      />
                    </g>
                  </svg>

                  <svg
                    aria-hidden="true"
                    className="af-theme-switch__backdrop"
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 290 120"
                  >
                    <g className="af-theme-switch__stars">
                      <g><path fill="#fff" fillRule="evenodd" d="M52 8a.6.6 0 0 1 .578.435l.65 2.276a3 3 0 0 0 2.061 2.061l2.276.65a.6.6 0 0 1 0 1.156l-2.276.65a3 3 0 0 0-2.061 2.061l-.65 2.276a.6.6 0 0 1-1.156 0l-.65-2.276a3 3 0 0 0-2.061-2.061l-2.276-.65a.6.6 0 0 1 0-1.156l2.276-.65A3 3 0 0 0 50.772 8.435L51.422 6.16A.6.6 0 0 1 52 8Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M53.5 36a.263.263 0 0 1 .252.19l.285.997a1.313 1.313 0 0 0 .902.901l.997.285a.263.263 0 0 1 0 .504l-.997.285a1.313 1.313 0 0 0-.902.901l-.285.997a.263.263 0 0 1-.504 0l-.285-.997a1.313 1.313 0 0 0-.902-.901l-.997-.285a.263.263 0 0 1 0-.504l.997-.285a1.313 1.313 0 0 0 .902-.901l.285-.997A.263.263 0 0 1 53.5 36Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M28 26a.224.224 0 0 1 .216.163l.244.854a1.123 1.123 0 0 0 .773.773l.854.244a.224.224 0 0 1 0 .432l-.854.244a1.123 1.123 0 0 0-.773.773l-.244.854a.224.224 0 0 1-.432 0l-.244-.854a1.123 1.123 0 0 0-.773-.773l-.854-.244a.224.224 0 0 1 0-.432l.854-.244a1.123 1.123 0 0 0 .773-.773l.244-.854A.225.225 0 0 1 28 26Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M36 62a.263.263 0 0 1 .252.19l.285.997a1.313 1.313 0 0 0 .902.901l.997.285a.263.263 0 0 1 0 .504l-.997.285a1.313 1.313 0 0 0-.902.901l-.285.997a.263.263 0 0 1-.504 0l-.285-.997a1.313 1.313 0 0 0-.902-.901l-.997-.285a.263.263 0 0 1 0-.504l.997-.285a1.313 1.313 0 0 0 .902-.901l.285-.997A.263.263 0 0 1 36 62Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M124 22a.6.6 0 0 1 .578.435l.65 2.276a3 3 0 0 0 2.061 2.061l2.276.65a.6.6 0 0 1 0 1.156l-2.276.65a3 3 0 0 0-2.061 2.061l-.65 2.276a.6.6 0 0 1-1.156 0l-.65-2.276a3 3 0 0 0-2.061-2.061l-2.276-.65a.6.6 0 0 1 0-1.156l2.276-.65a3 3 0 0 0 2.061-2.061l.65-2.276A.6.6 0 0 1 124 22Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M100 52a.3.3 0 0 1 .288.217l.325 1.139a1.5 1.5 0 0 0 1.031 1.031l1.139.325a.3.3 0 0 1 0 .576l-1.139.325a1.5 1.5 0 0 0-1.031 1.031l-.325 1.139a.3.3 0 0 1-.576 0l-.325-1.139a1.5 1.5 0 0 0-1.031-1.031l-1.139-.325a.3.3 0 0 1 0-.576l1.139-.325A1.5 1.5 0 0 0 99.387 53l.325-1.139A.3.3 0 0 1 100 52Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M115 68a.41.41 0 0 1 .397.299l.447 1.565a2.057 2.057 0 0 0 1.416 1.416l1.565.447a.411.411 0 0 1 0 .792l-1.565.447a2.057 2.057 0 0 0-1.416 1.416l-.447 1.565a.411.411 0 0 1-.794 0l-.447-1.565a2.057 2.057 0 0 0-1.416-1.416l-1.565-.447a.411.411 0 0 1 0-.792l1.565-.447a2.057 2.057 0 0 0 1.416-1.416l.447-1.565A.41.41 0 0 1 115 68Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M165 16a.263.263 0 0 1 .252.19l.285.997a1.313 1.313 0 0 0 .902.901l.997.285a.263.263 0 0 1 0 .504l-.997.285a1.313 1.313 0 0 0-.902.901l-.285.997a.263.263 0 0 1-.504 0l-.285-.997a1.313 1.313 0 0 0-.902-.901l-.997-.285a.263.263 0 0 1 0-.504l.997-.285a1.313 1.313 0 0 0 .902-.901l.285-.997A.263.263 0 0 1 165 16Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M188 8a.6.6 0 0 1 .578.435l.65 2.276a3 3 0 0 0 2.061 2.061l2.276.65a.6.6 0 0 1 0 1.156l-2.276.65a3 3 0 0 0-2.061 2.061l-.65 2.276a.6.6 0 0 1-1.156 0l-.65-2.276a3 3 0 0 0-2.061-2.061l-2.276-.65a.6.6 0 0 1 0-1.156l2.276-.65a3 3 0 0 0 2.061-2.061l.65-2.276A.6.6 0 0 1 188 8Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M178 36a.3.3 0 0 1 .288.217l.325 1.139a1.5 1.5 0 0 0 1.031 1.031l1.139.325a.3.3 0 0 1 0 .576l-1.139.325a1.5 1.5 0 0 0-1.031 1.031l-.325 1.139a.3.3 0 0 1-.576 0l-.325-1.139a1.5 1.5 0 0 0-1.031-1.031l-1.139-.325a.3.3 0 0 1 0-.576l1.139-.325A1.5 1.5 0 0 0 178.613 37l.325-1.139A.3.3 0 0 1 178 36Z" clipRule="evenodd" /></g>
                      <g><path fill="#fff" fillRule="evenodd" d="M155 52a.41.41 0 0 1 .397.299l.447 1.565a2.057 2.057 0 0 0 1.416 1.416l1.565.447a.411.411 0 0 1 0 .792l-1.565.447a2.057 2.057 0 0 0-1.416 1.416l-.447 1.565a.411.411 0 0 1-.794 0l-.447-1.565a2.057 2.057 0 0 0-1.416-1.416l-1.565-.447a.411.411 0 0 1 0-.792l1.565-.447a2.057 2.057 0 0 0 1.416-1.416l.447-1.565A.41.41 0 0 1 155 52Z" clipRule="evenodd" /></g>
                    </g>
                  </svg>

                  <svg
                    aria-hidden="true"
                    className="af-theme-switch__backdrop"
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 290 120"
                  >
                    <g>
                      <path
                        fill="rgba(255,255,255,0.97)"
                        d="M290 88c0 11.6-6.1 21.8-15.3 27.5l.008.71c0 17.9-14.5 32.4-32.4 32.4a32.6 32.6 0 0 1-8.36-1.09C228.3 155.6 215.8 166 201 166a32.6 32.6 0 0 1-5.88-.532C189.2 170.4 180.8 174 171.5 174a32.3 32.3 0 0 1-21.26-7.95A32.3 32.3 0 0 1 139.5 168a32.3 32.3 0 0 1-9.56-1.43C123.3 172.6 114.4 176 104.5 176a32.3 32.3 0 0 1-12.06-2.32A32.3 32.3 0 0 1 70.5 178c-17.9 0-32.4-14.5-32.4-32.4s14.5-32.4 32.4-32.4a32.3 32.3 0 0 1 12.06 2.32A32.3 32.3 0 0 1 104.5 111c3.33 0 6.54.5 9.56 1.43C120.7 106.4 129.6 103 139.5 103a32.3 32.3 0 0 1 21.26 7.95A32.3 32.3 0 0 1 171.5 109c2.01 0 3.98.182 5.88.532C183.3 104.6 191.8 101 201 101c2.89 0 5.69.379 8.36 1.09 2.16-8.01 7.3-14.79 14.16-19.08l-.008-.71C223.508 65 237.5 53 253.5 53S290 67.5 290 88Z"
                      />
                    </g>
                  </svg>

                  <span className="af-theme-switch__indicator-wrap">
                    <span className="af-theme-switch__indicator">
                      <span className="af-theme-switch__sphere">
                        <span className="af-theme-switch__sun">
                          <span className="af-theme-switch__moon">
                            <span className="af-theme-switch__crater" />
                            <span className="af-theme-switch__crater" />
                            <span className="af-theme-switch__crater" />
                          </span>
                        </span>
                      </span>
                    </span>
                  </span>
                </span>
              </button>
            </div>
          </div>
        </div>
      </aside>

      <main className="af-main-shell min-h-0 min-w-0 flex-1">
        <Outlet />
      </main>
    </div>
  )
}
