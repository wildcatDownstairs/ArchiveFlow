import { NavLink, Outlet } from "react-router-dom"
import { useTranslation } from "react-i18next"
import { Home, ListTodo, FileText, Settings } from "lucide-react"
import { cn } from "@/lib/utils"

const navItems = [
  { to: "/", icon: Home, labelKey: "home" },
  { to: "/tasks", icon: ListTodo, labelKey: "tasks" },
  { to: "/reports", icon: FileText, labelKey: "reports" },
  { to: "/settings", icon: Settings, labelKey: "settings" },
] as const

export default function MainLayout() {
  const { t } = useTranslation()

  return (
    <div className="flex h-screen">
      {/* Sidebar */}
      <aside className="w-60 flex-shrink-0 bg-gray-900 text-white flex flex-col">
        <div className="p-5 text-xl font-bold tracking-wide">
          ArchiveFlow
        </div>

        <nav className="flex-1 flex flex-col gap-1 px-3">
          {navItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                  isActive
                    ? "bg-gray-700 text-white"
                    : "text-gray-400 hover:bg-gray-800 hover:text-white"
                )
              }
            >
              <item.icon className="h-5 w-5" />
              {t(item.labelKey)}
            </NavLink>
          ))}
        </nav>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-auto bg-background">
        <Outlet />
      </main>
    </div>
  )
}
