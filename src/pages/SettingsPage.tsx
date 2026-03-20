import { useCallback, useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { Globe, Trash2, Database, Info, ExternalLink, Languages } from "lucide-react"
import { useAppStore } from "@/stores/appStore"
import {
  getAppDataDir,
  clearAllTasks,
  clearAuditEvents,
  getStats,
  recordSettingChange,
} from "@/services/api"

export default function SettingsPage() {
  const { t, i18n } = useTranslation()
  const setLocale = useAppStore((s) => s.setLocale)

  const [appDataDir, setAppDataDir] = useState("")
  const [taskCount, setTaskCount] = useState(0)
  const [auditCount, setAuditCount] = useState(0)
  const [confirmAction, setConfirmAction] = useState<"tasks" | "audit" | null>(null)

  const loadStats = useCallback(async () => {
    try {
      const [tasks, audits] = await getStats()
      setTaskCount(tasks)
      setAuditCount(audits)
    } catch {
      // Ignore stats loading errors in the settings UI.
    }
  }, [])

  useEffect(() => {
    getAppDataDir()
      .then((dir) => setAppDataDir(dir))
      .catch(() => {})
    getStats()
      .then(([tasks, audits]) => {
        setTaskCount(tasks)
        setAuditCount(audits)
      })
      .catch(() => {})
  }, [])

  // Language switching
  const handleLanguageChange = async (lng: string) => {
    if (lng === currentLang) {
      return
    }

    void i18n.changeLanguage(lng)
    setLocale(lng)

    try {
      await recordSettingChange("language", currentLang, lng)
    } catch {
      // Ignore audit write failures in the settings UI.
    }
  }

  // Clear tasks with confirmation
  const handleClearTasks = async () => {
    try {
      await clearAllTasks()
      await loadStats()
      setConfirmAction(null)
    } catch {
      // Ignore
    }
  }

  // Clear audit logs with confirmation
  const handleClearAudit = async () => {
    try {
      await clearAuditEvents()
      await loadStats()
      setConfirmAction(null)
    } catch {
      // Ignore
    }
  }

  const currentLang = i18n.language

  return (
    <div className="p-6 space-y-8 max-w-2xl">
      <h1 className="text-2xl font-bold">{t("settings")}</h1>

      {/* Section 1: Language */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Globe className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-lg font-semibold">{t("language")}</h2>
        </div>
        <div className="rounded-lg border p-4 space-y-3">
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="radio"
              name="language"
              value="zh"
              checked={currentLang === "zh"}
              onChange={() => void handleLanguageChange("zh")}
              className="h-4 w-4 accent-primary"
            />
            <Languages className="h-4 w-4 text-muted-foreground" />
            <span>{t("language_zh")}</span>
          </label>
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="radio"
              name="language"
              value="en"
              checked={currentLang === "en"}
              onChange={() => void handleLanguageChange("en")}
              className="h-4 w-4 accent-primary"
            />
            <Languages className="h-4 w-4 text-muted-foreground" />
            <span>{t("language_en")}</span>
          </label>
        </div>
      </section>

      {/* Section 2: Data Management */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Database className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-lg font-semibold">{t("data_management")}</h2>
        </div>
        <div className="rounded-lg border p-4 space-y-4">
          {/* Data directory */}
          {appDataDir && (
            <div className="flex items-start gap-2 text-sm">
              <span className="text-muted-foreground shrink-0">{t("app_data_dir")}:</span>
              <span className="break-all font-mono text-xs">{appDataDir}</span>
            </div>
          )}

          {/* Stats */}
          <div className="flex gap-6 text-sm">
            <span>
              {t("task_count")}: <strong>{taskCount}</strong>
            </span>
            <span>
              {t("audit_count")}: <strong>{auditCount}</strong>
            </span>
          </div>

          {/* Clear tasks button */}
          <div className="space-y-2">
            {confirmAction === "tasks" ? (
              <div className="rounded-md border border-destructive/50 bg-destructive/5 p-3 space-y-2">
                <p className="text-sm text-destructive">{t("clear_tasks_confirm")}</p>
                <div className="flex gap-2">
                  <button
                    onClick={() => void handleClearTasks()}
                    className="px-3 py-1.5 text-sm rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90"
                  >
                    {t("clear_all_tasks")}
                  </button>
                  <button
                    onClick={() => setConfirmAction(null)}
                    className="px-3 py-1.5 text-sm rounded-md border hover:bg-muted"
                  >
                    {t("cancel")}
                  </button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setConfirmAction("tasks")}
                disabled={taskCount === 0}
                className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Trash2 className="h-4 w-4" />
                {t("clear_all_tasks")}
              </button>
            )}
          </div>

          {/* Clear audit logs button */}
          <div className="space-y-2">
            {confirmAction === "audit" ? (
              <div className="rounded-md border border-destructive/50 bg-destructive/5 p-3 space-y-2">
                <p className="text-sm text-destructive">{t("clear_audit_confirm")}</p>
                <div className="flex gap-2">
                  <button
                    onClick={() => void handleClearAudit()}
                    className="px-3 py-1.5 text-sm rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90"
                  >
                    {t("clear_audit_logs")}
                  </button>
                  <button
                    onClick={() => setConfirmAction(null)}
                    className="px-3 py-1.5 text-sm rounded-md border hover:bg-muted"
                  >
                    {t("cancel")}
                  </button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setConfirmAction("audit")}
                disabled={auditCount === 0}
                className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Trash2 className="h-4 w-4" />
                {t("clear_audit_logs")}
              </button>
            )}
          </div>
        </div>
      </section>

      {/* Section 3: About */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Info className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-lg font-semibold">{t("about")}</h2>
        </div>
        <div className="rounded-lg border p-4 space-y-3 text-sm">
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">{t("version")}:</span>
            <span className="font-mono">v0.1.0</span>
          </div>
          <div className="flex items-start gap-2">
            <span className="text-muted-foreground shrink-0">{t("tech_stack")}:</span>
            <span>Tauri 2 + React + TypeScript + Rust + SQLite</span>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">{t("github_repo")}:</span>
            <a
              href="https://github.com/wildcatDownstairs/ArchiveFlow"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-primary hover:underline"
            >
              ArchiveFlow
              <ExternalLink className="h-3 w-3" />
            </a>
          </div>
        </div>
      </section>
    </div>
  )
}
