import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import {
  Globe,
  Trash2,
  Database,
  Info,
  ExternalLink,
  Languages,
  SlidersHorizontal,
} from "lucide-react"
import {
  useAppStore,
  type CharsetFlags,
  type ResultRetentionPolicy,
} from "@/stores/appStore"
import {
  getAppDataDir,
  clearAllTasks,
  clearAuditEvents,
  getStats,
  recordSettingChange,
} from "@/services/api"

function stringifySetting(value: unknown): string {
  if (typeof value === "string") return value
  return JSON.stringify(value)
}

export default function SettingsPage() {
  const { t, i18n } = useTranslation()
  const setLocale = useAppStore((s) => s.setLocale)
  const recoveryPreferences = useAppStore((s) => s.recoveryPreferences)
  const updateRecoveryPreferences = useAppStore((s) => s.updateRecoveryPreferences)

  const [appDataDir, setAppDataDir] = useState("")
  const [taskCount, setTaskCount] = useState(0)
  const [auditCount, setAuditCount] = useState(0)
  const [confirmAction, setConfirmAction] = useState<"tasks" | "audit" | null>(null)

  const loadStats = async () => {
    try {
      const [tasks, audits] = await getStats()
      setTaskCount(tasks)
      setAuditCount(audits)
    } catch {
      // Ignore stats loading errors in the settings UI.
    }
  }

  useEffect(() => {
    let isMounted = true

    getAppDataDir()
      .then((dir) => {
        if (isMounted) {
          setAppDataDir(dir)
        }
      })
      .catch(() => {})

    getStats()
      .then(([tasks, audits]) => {
        if (isMounted) {
          setTaskCount(tasks)
          setAuditCount(audits)
        }
      })
      .catch(() => {})

    return () => {
      isMounted = false
    }
  }, [])

  const persistSettingChange = async (key: string, oldValue: unknown, newValue: unknown) => {
    try {
      await recordSettingChange(
        key,
        stringifySetting(oldValue),
        stringifySetting(newValue),
      )
    } catch {
      // Ignore audit write failures in the settings UI.
    }
  }

  const handleLanguageChange = async (lng: string) => {
    if (lng === currentLang) return

    void i18n.changeLanguage(lng)
    setLocale(lng)
    await persistSettingChange("language", currentLang, lng)
  }

  const handleCharsetFlagChange = async (key: keyof CharsetFlags, value: boolean) => {
    const oldFlags = recoveryPreferences.defaultCharsetFlags
    if (oldFlags[key] === value) return

    const nextFlags = { ...oldFlags, [key]: value }
    updateRecoveryPreferences({ defaultCharsetFlags: nextFlags })
    await persistSettingChange(
      `recovery.default_charset_flags.${key}`,
      oldFlags[key],
      value,
    )
  }

  const handleNumericPreferenceChange = async (
    key: "defaultMinLength" | "defaultMaxLength",
    value: number,
  ) => {
    const normalized = Math.max(1, value)
    if (recoveryPreferences[key] === normalized) return

    updateRecoveryPreferences({ [key]: normalized })
    await persistSettingChange(`recovery.${key}`, recoveryPreferences[key], normalized)
  }

  const handleBooleanPreferenceChange = async (
    key: "autoIncludeFilenamePatterns" | "autoClearDictionaryInput",
    value: boolean,
  ) => {
    if (recoveryPreferences[key] === value) return

    updateRecoveryPreferences({ [key]: value })
    await persistSettingChange(`recovery.${key}`, recoveryPreferences[key], value)
  }

  const handleRetentionPolicyChange = async (value: ResultRetentionPolicy) => {
    if (recoveryPreferences.resultRetentionPolicy === value) return

    updateRecoveryPreferences({ resultRetentionPolicy: value })
    await persistSettingChange(
      "recovery.resultRetentionPolicy",
      recoveryPreferences.resultRetentionPolicy,
      value,
    )
  }

  const handleClearTasks = async () => {
    try {
      await clearAllTasks()
      await loadStats()
      setConfirmAction(null)
    } catch {
      // Ignore
    }
  }

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
    <div className="p-6 space-y-8 max-w-3xl">
      <h1 className="text-2xl font-bold">{t("settings")}</h1>

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

      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <SlidersHorizontal className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-lg font-semibold">{t("recovery_defaults")}</h2>
        </div>
        <div className="rounded-lg border p-4 space-y-5">
          <div className="space-y-2">
            <p className="text-sm font-medium">{t("default_charset")}</p>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-sm">
              {(
                [
                  ["lowercase", t("charset_lowercase")],
                  ["uppercase", t("charset_uppercase")],
                  ["digits", t("charset_digits")],
                  ["special", t("charset_special")],
                ] as const
              ).map(([key, label]) => (
                <label key={key} className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={recoveryPreferences.defaultCharsetFlags[key]}
                    onChange={(e) => void handleCharsetFlagChange(key, e.target.checked)}
                    className="rounded border-gray-300"
                  />
                  {label}
                </label>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <label className="text-sm font-medium">{t("default_min_length")}</label>
              <input
                type="number"
                value={recoveryPreferences.defaultMinLength}
                min={1}
                max={16}
                onChange={(e) =>
                  void handleNumericPreferenceChange(
                    "defaultMinLength",
                    parseInt(e.target.value, 10) || 1,
                  )
                }
                className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm font-medium">{t("default_max_length")}</label>
              <input
                type="number"
                value={recoveryPreferences.defaultMaxLength}
                min={1}
                max={16}
                onChange={(e) =>
                  void handleNumericPreferenceChange(
                    "defaultMaxLength",
                    parseInt(e.target.value, 10) || 1,
                  )
                }
                className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
          </div>

          <div className="space-y-2 text-sm">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={recoveryPreferences.autoIncludeFilenamePatterns}
                onChange={(e) =>
                  void handleBooleanPreferenceChange(
                    "autoIncludeFilenamePatterns",
                    e.target.checked,
                  )
                }
                className="rounded border-gray-300"
              />
              {t("include_filename_patterns")}
            </label>
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={recoveryPreferences.autoClearDictionaryInput}
                onChange={(e) =>
                  void handleBooleanPreferenceChange(
                    "autoClearDictionaryInput",
                    e.target.checked,
                  )
                }
                className="rounded border-gray-300"
              />
              {t("auto_clear_dictionary_input")}
            </label>
          </div>

          <div className="space-y-2">
            <p className="text-sm font-medium">{t("result_retention_policy")}</p>
            <label className="flex items-center gap-2 text-sm cursor-pointer">
              <input
                type="radio"
                name="resultRetentionPolicy"
                checked={recoveryPreferences.resultRetentionPolicy === "plaintext"}
                onChange={() => void handleRetentionPolicyChange("plaintext")}
                className="h-4 w-4 accent-primary"
              />
              {t("retention_plaintext")}
            </label>
            <label className="flex items-center gap-2 text-sm cursor-pointer">
              <input
                type="radio"
                name="resultRetentionPolicy"
                checked={recoveryPreferences.resultRetentionPolicy === "masked"}
                onChange={() => void handleRetentionPolicyChange("masked")}
                className="h-4 w-4 accent-primary"
              />
              {t("retention_masked")}
            </label>
          </div>
        </div>
      </section>

      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Database className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-lg font-semibold">{t("data_management")}</h2>
        </div>
        <div className="rounded-lg border p-4 space-y-4">
          {appDataDir && (
            <div className="flex items-start gap-2 text-sm">
              <span className="text-muted-foreground shrink-0">{t("app_data_dir")}:</span>
              <span className="break-all font-mono text-xs">{appDataDir}</span>
            </div>
          )}

          <div className="flex gap-6 text-sm">
            <span>
              {t("task_count")}: <strong>{taskCount}</strong>
            </span>
            <span>
              {t("audit_count")}: <strong>{auditCount}</strong>
            </span>
          </div>

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
