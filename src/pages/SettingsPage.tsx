import { useEffect, useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import {
  Globe,
  Trash2,
  Database,
  Info,
  ExternalLink,
  SlidersHorizontal,
  Search,
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
  detectHashcat,
  getStats,
  recordSettingChange,
  setRecoverySchedulerLimit,
} from "@/services/api"
import { cn } from "@/lib/utils"
import { DANGER_BUTTON_CLASS, GHOST_BUTTON_CLASS } from "@/lib/ui"
import type { HashcatDetectionResult } from "@/types"

function stringifySetting(value: unknown): string {
  if (typeof value === "string") return value
  return JSON.stringify(value)
}

export default function SettingsPage() {
  const { t, i18n } = useTranslation()
  const setLocale = useAppStore((state) => state.setLocale)
  const recoveryPreferences = useAppStore((state) => state.recoveryPreferences)
  const updateRecoveryPreferences = useAppStore((state) => state.updateRecoveryPreferences)

  const [appDataDir, setAppDataDir] = useState("")
  const [taskCount, setTaskCount] = useState(0)
  const [auditCount, setAuditCount] = useState(0)
  const [confirmAction, setConfirmAction] = useState<"tasks" | "audit" | null>(null)
  const [hashcatPathInput, setHashcatPathInput] = useState(recoveryPreferences.hashcatPath)
  const [hashcatStatus, setHashcatStatus] = useState<HashcatDetectionResult | null>(null)
  const [hashcatChecking, setHashcatChecking] = useState(false)

  const loadStats = async () => {
    try {
      const [tasks, audits] = await getStats()
      setTaskCount(tasks)
      setAuditCount(audits)
    } catch {
      // Ignore stats loading failures in the settings view.
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

  useEffect(() => {
    let cancelled = false

    const preloadHashcat = async () => {
      if (recoveryPreferences.hashcatPath.trim()) {
        return
      }

      try {
        const result = await detectHashcat()
        if (cancelled) return
        setHashcatStatus(result)
        if (result.path) {
          setHashcatPathInput(result.path)
          updateRecoveryPreferences({ hashcatPath: result.path })
        }
      } catch {
        if (!cancelled) {
          setHashcatStatus(null)
        }
      }
    }

    void preloadHashcat()

    return () => {
      cancelled = true
    }
  }, [recoveryPreferences.hashcatPath, updateRecoveryPreferences])

  const persistSettingChange = async (
    key: string,
    oldValue: unknown,
    newValue: unknown,
  ) => {
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

  const currentLang = i18n.language

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
    key: "defaultMinLength" | "defaultMaxLength" | "defaultTaskPriority",
    value: number,
  ) => {
    const normalized = key === "defaultTaskPriority" ? value : Math.max(1, value)
    if (recoveryPreferences[key] === normalized) return

    updateRecoveryPreferences({ [key]: normalized })
    await persistSettingChange(`recovery.${key}`, recoveryPreferences[key], normalized)
  }

  const handleBooleanPreferenceChange = async (
    key:
      | "autoIncludeFilenamePatterns"
      | "autoClearDictionaryInput"
      | "exportMaskPasswords"
      | "exportIncludeAuditEvents",
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

  const handleSchedulerLimitChange = async (value: number) => {
    const normalized = Math.max(1, value)
    if (recoveryPreferences.maxConcurrentRecoveries === normalized) return

    updateRecoveryPreferences({ maxConcurrentRecoveries: normalized })
    try {
      await setRecoverySchedulerLimit(normalized)
    } catch {
      // Ignore runtime sync failures and keep the local preference.
    }
    await persistSettingChange(
      "recovery.maxConcurrentRecoveries",
      recoveryPreferences.maxConcurrentRecoveries,
      normalized,
    )
  }

  const handleHashcatPathCommit = async () => {
    const normalized = hashcatPathInput.trim()
    if (recoveryPreferences.hashcatPath === normalized) {
      setHashcatPathInput(normalized)
      return
    }

    updateRecoveryPreferences({ hashcatPath: normalized })
    setHashcatPathInput(normalized)
    await persistSettingChange("recovery.hashcatPath", recoveryPreferences.hashcatPath, normalized)
  }

  const handleDetectHashcat = async () => {
    setHashcatChecking(true)
    try {
      const result = await detectHashcat(hashcatPathInput)
      setHashcatStatus(result)
      if (result.path && result.path !== recoveryPreferences.hashcatPath) {
        updateRecoveryPreferences({ hashcatPath: result.path })
        setHashcatPathInput(result.path)
      }
    } catch (error) {
      setHashcatStatus({
        available: false,
        path: hashcatPathInput || null,
        version: null,
        devices: [],
        error: String(error),
      })
    } finally {
      setHashcatChecking(false)
    }
  }

  const handleClearTasks = async () => {
    try {
      await clearAllTasks()
      await loadStats()
      setConfirmAction(null)
    } catch {
      // Ignore clear failures in this UI.
    }
  }

  const handleClearAudit = async () => {
    try {
      await clearAuditEvents()
      await loadStats()
      setConfirmAction(null)
    } catch {
      // Ignore clear failures in this UI.
    }
  }

  return (
    <div className="af-page af-scrollbar-none overflow-y-auto">
      <div className="mx-auto max-w-[1240px]">
        <div className="border-b border-white/6 pb-5">
          <h1 className="af-page-title">{t("settings")}</h1>
        </div>

        <div className="mt-6 grid gap-4 xl:grid-cols-[minmax(0,1.25fr)_minmax(360px,0.9fr)]">
          <div className="space-y-4">
            <SettingsCard
              icon={<SlidersHorizontal className="h-4 w-4" />}
              title={t("recovery_defaults")}
            >
              <CardRow>
                <FieldLabel>{t("default_charset")}</FieldLabel>
                <div className="flex flex-wrap gap-2">
                  {(
                    [
                      ["lowercase", t("charset_lowercase")],
                      ["uppercase", t("charset_uppercase")],
                      ["digits", t("charset_digits")],
                      ["special", t("charset_special")],
                    ] as const
                  ).map(([key, label]) => (
                    <CharsetTag
                      key={key}
                      label={label}
                      checked={recoveryPreferences.defaultCharsetFlags[key]}
                      onChange={(checked) =>
                        void handleCharsetFlagChange(key, checked)
                      }
                    />
                  ))}
                </div>
              </CardRow>

              <SplitRow>
                <SplitCell>
                  <FieldLabel>{t("default_min_length")}</FieldLabel>
                  <NumberInput
                    value={recoveryPreferences.defaultMinLength}
                    min={1}
                    max={16}
                    onChange={(value) =>
                      void handleNumericPreferenceChange("defaultMinLength", value)
                    }
                  />
                </SplitCell>
                <SplitCell>
                  <FieldLabel>{t("default_max_length")}</FieldLabel>
                  <NumberInput
                    value={recoveryPreferences.defaultMaxLength}
                    min={1}
                    max={16}
                    onChange={(value) =>
                      void handleNumericPreferenceChange("defaultMaxLength", value)
                    }
                  />
                </SplitCell>
              </SplitRow>

              <CardRow horizontal>
                <FieldLabel compact>{t("default_task_priority")}</FieldLabel>
                <div className="flex items-center gap-3">
                  <Stepper
                    value={recoveryPreferences.defaultTaskPriority}
                    onChange={(value) =>
                      void handleNumericPreferenceChange("defaultTaskPriority", value)
                    }
                    min={-10}
                    max={10}
                  />
                  <span className="max-w-[132px] text-[11px] leading-5 text-muted-foreground">
                    {t("default_task_priority_hint")}
                  </span>
                </div>
              </CardRow>

              <SplitRow>
                <SplitCell>
                  <FieldLabel>{t("max_concurrent_recoveries")}</FieldLabel>
                  <NumberInput
                    value={recoveryPreferences.maxConcurrentRecoveries}
                    min={1}
                    max={32}
                    onChange={(value) => void handleSchedulerLimitChange(value)}
                  />
                </SplitCell>
                <SplitCell>
                  <FieldLabel>{t("hashcat_path")}</FieldLabel>
                  <input
                    type="text"
                    value={hashcatPathInput}
                    placeholder={t("hashcat_path_placeholder")}
                    onChange={(event) => setHashcatPathInput(event.target.value)}
                    onBlur={() => void handleHashcatPathCommit()}
                    className="af-input font-mono text-xs"
                    title={hashcatPathInput}
                  />
                  <button
                    type="button"
                    onClick={() => void handleDetectHashcat()}
                    disabled={hashcatChecking}
                    className={`${GHOST_BUTTON_CLASS} mt-2 px-3 py-1.5 text-xs`}
                  >
                    <Search className="h-3.5 w-3.5" />
                    {hashcatChecking ? t("loading") : t("detect_hashcat")}
                  </button>
                  {hashcatStatus && (
                    <HashcatStatusPanel status={hashcatStatus} />
                  )}
                </SplitCell>
              </SplitRow>

              <SplitRow>
                <SplitCell className="p-0">
                  <CheckItem
                    checked={recoveryPreferences.autoIncludeFilenamePatterns}
                    onChange={(checked) =>
                      void handleBooleanPreferenceChange("autoIncludeFilenamePatterns", checked)
                    }
                    label={t("include_filename_patterns")}
                  />
                </SplitCell>
                <SplitCell className="p-0">
                  <CheckItem
                    checked={recoveryPreferences.autoClearDictionaryInput}
                    onChange={(checked) =>
                      void handleBooleanPreferenceChange("autoClearDictionaryInput", checked)
                    }
                    label={t("auto_clear_dictionary_input")}
                  />
                </SplitCell>
              </SplitRow>

              <SplitRow noBorder>
                <SplitCell>
                  <FieldLabel>{t("result_retention_policy")}</FieldLabel>
                  <div className="space-y-1">
                    <RadioItem
                      name="retention"
                      checked={recoveryPreferences.resultRetentionPolicy === "plaintext"}
                      onChange={() => void handleRetentionPolicyChange("plaintext")}
                      label={t("retention_plaintext")}
                      compact
                    />
                    <RadioItem
                      name="retention"
                      checked={recoveryPreferences.resultRetentionPolicy === "masked"}
                      onChange={() => void handleRetentionPolicyChange("masked")}
                      label={t("retention_masked")}
                      compact
                    />
                  </div>
                </SplitCell>
                <SplitCell>
                  <FieldLabel>{t("export_defaults")}</FieldLabel>
                  <div className="space-y-1">
                    <CheckItem
                      checked={recoveryPreferences.exportMaskPasswords}
                      onChange={(checked) =>
                        void handleBooleanPreferenceChange("exportMaskPasswords", checked)
                      }
                      label={t("export_mask_passwords")}
                      compact
                    />
                    <CheckItem
                      checked={recoveryPreferences.exportIncludeAuditEvents}
                      onChange={(checked) =>
                        void handleBooleanPreferenceChange("exportIncludeAuditEvents", checked)
                      }
                      label={t("export_include_audit_events")}
                      compact
                    />
                  </div>
                </SplitCell>
              </SplitRow>
            </SettingsCard>
          </div>

          <div className="space-y-4">
            <SettingsCard
              icon={<Globe className="h-4 w-4" />}
              title={t("language")}
            >
              <RadioItem
                name="language"
                checked={currentLang === "zh"}
                onChange={() => void handleLanguageChange("zh")}
                label={t("language_zh")}
                prefix={<span className="text-sm">🇨🇳</span>}
              />
              <RadioItem
                name="language"
                checked={currentLang === "en"}
                onChange={() => void handleLanguageChange("en")}
                label={t("language_en")}
                prefix={<span className="text-sm">🇺🇸</span>}
                noBorder
              />
            </SettingsCard>

            <SettingsCard
              icon={<Database className="h-4 w-4" />}
              title={t("data_management")}
            >
              {appDataDir && (
                <div className="border-b border-white/6 px-5 py-3 font-mono text-[11.5px] leading-5 text-muted-foreground break-all">
                  {appDataDir}
                </div>
              )}

              <div className="flex gap-7 border-b border-white/6 px-5 py-4">
                <StatBlock label={t("task_count")} value={taskCount} />
                <StatBlock label={t("audit_count")} value={auditCount} />
              </div>

              <div className="px-5 py-4">
                <div className="flex flex-wrap gap-2">
                  {confirmAction === "tasks" ? (
                    <ConfirmDanger
                      text={t("clear_tasks_confirm")}
                      confirmLabel={t("clear_all_tasks")}
                      cancelLabel={t("cancel")}
                      onConfirm={() => void handleClearTasks()}
                      onCancel={() => setConfirmAction(null)}
                    />
                  ) : (
                    <button
                      onClick={() => setConfirmAction("tasks")}
                      disabled={taskCount === 0}
                      className={`${DANGER_BUTTON_CLASS} px-3.5 py-2 text-xs ${taskCount === 0 ? "pointer-events-none opacity-50" : ""}`}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      {t("clear_all_tasks")}
                    </button>
                  )}

                  {confirmAction === "audit" ? (
                    <ConfirmDanger
                      text={t("clear_audit_confirm")}
                      confirmLabel={t("clear_audit_logs")}
                      cancelLabel={t("cancel")}
                      onConfirm={() => void handleClearAudit()}
                      onCancel={() => setConfirmAction(null)}
                    />
                  ) : (
                    <button
                      onClick={() => setConfirmAction("audit")}
                      disabled={auditCount === 0}
                      className={`${DANGER_BUTTON_CLASS} px-3.5 py-2 text-xs ${auditCount === 0 ? "pointer-events-none opacity-50" : ""}`}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      {t("clear_audit_logs")}
                    </button>
                  )}
                </div>
              </div>
            </SettingsCard>

            <SettingsCard
              icon={<Info className="h-4 w-4" />}
              title={t("about")}
            >
              <AboutRow
                label={t("version")}
                value={<span className="font-mono text-[13px]">v0.1.0</span>}
              />
              <AboutRow
                label={t("tech_stack")}
                value={
                  <span className="text-[12.5px] text-muted-foreground">
                    Tauri 2 + React + TypeScript + Rust + SQLite
                  </span>
                }
              />
              <AboutRow
                label={t("github_repo")}
                value={
                  <a
                    href="https://github.com/wildcatDownstairs/ArchiveFlow"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1 text-primary transition-colors hover:text-primary/80"
                  >
                    ArchiveFlow
                    <ExternalLink className="h-3 w-3" />
                  </a>
                }
                noBorder
              />
            </SettingsCard>
          </div>
        </div>
      </div>
    </div>
  )
}

function SettingsCard({
  icon,
  title,
  children,
}: {
  icon: ReactNode
  title: string
  children: ReactNode
}) {
  return (
    <section className="overflow-hidden rounded-[14px] border border-white/6 bg-card">
      <div className="flex items-center gap-2 border-b border-white/6 px-5 py-4">
        <span className="text-primary">{icon}</span>
        <h2 className="af-display text-[13px] font-bold text-foreground">{title}</h2>
      </div>
      {children}
    </section>
  )
}

function CardRow({
  children,
  horizontal = false,
}: {
  children: ReactNode
  horizontal?: boolean
}) {
  return (
    <div
      className={cn(
        "border-b border-white/6 px-5 py-4",
        horizontal && "flex items-center justify-between gap-4",
      )}
    >
      {children}
    </div>
  )
}

function SplitRow({
  children,
  noBorder = false,
}: {
  children: ReactNode
  noBorder?: boolean
}) {
  return (
    <div
      className={cn(
        "grid md:grid-cols-2",
        !noBorder && "border-b border-white/6",
      )}
    >
      {children}
    </div>
  )
}

function SplitCell({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  return (
    <div className={cn("px-5 py-4 md:border-l md:first:border-l-0 md:border-white/6", className)}>
      {children}
    </div>
  )
}

function FieldLabel({
  children,
  compact = false,
}: {
  children: ReactNode
  compact?: boolean
}) {
  return (
    <div className={cn("af-kicker mb-2", compact && "mb-0")}>
      {children}
    </div>
  )
}

function CharsetTag({
  label,
  checked,
  onChange,
}: {
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-center gap-2 rounded-[8px] px-3 py-2 transition-colors",
        checked
          ? "bg-primary/15 text-primary"
          : "bg-secondary text-muted-foreground hover:bg-accent hover:text-foreground",
      )}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="sr-only"
      />
      <span
        className={cn(
          "h-[5px] w-[5px] rounded-full",
          checked ? "bg-primary" : "bg-muted-foreground/45",
        )}
      />
      <span className="text-[12.5px]">{label}</span>
    </label>
  )
}

function NumberInput({
  value,
  min,
  max,
  onChange,
}: {
  value: number
  min: number
  max: number
  onChange: (value: number) => void
}) {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      onChange={(event) => onChange(parseInt(event.target.value, 10) || min)}
      className="af-input text-sm"
    />
  )
}

function Stepper({
  value,
  onChange,
  min,
  max,
}: {
  value: number
  onChange: (value: number) => void
  min: number
  max: number
}) {
  return (
    <div className="inline-flex items-center overflow-hidden rounded-[8px] bg-secondary">
      <button
        type="button"
        className="flex h-[34px] w-8 items-center justify-center text-base text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onClick={() => onChange(Math.max(min, value - 1))}
      >
        −
      </button>
      <span className="min-w-[34px] text-center text-sm font-medium text-foreground">
        {value}
      </span>
      <button
        type="button"
        className="flex h-[34px] w-8 items-center justify-center text-base text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onClick={() => onChange(Math.min(max, value + 1))}
      >
        +
      </button>
    </div>
  )
}

function CheckItem({
  checked,
  onChange,
  label,
  compact = false,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  compact?: boolean
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-center gap-3 px-5 py-3 transition-colors hover:bg-white/[0.02]",
        !compact && "border-b border-white/6 last:border-b-0",
        compact && "px-0 py-2",
      )}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="sr-only"
      />
      <span
        className={cn(
          "flex h-4 w-4 items-center justify-center rounded-[4px] transition-colors",
          checked ? "bg-primary" : "bg-secondary",
        )}
      >
        {checked && (
          <svg width="7" height="5" viewBox="0 0 9 6" fill="none">
            <path
              d="M1 3L3.5 5.5L8 1"
              stroke="white"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </span>
      <span className={cn("text-[13px]", checked ? "text-foreground" : "text-muted-foreground")}>
        {label}
      </span>
    </label>
  )
}

function RadioItem({
  name,
  checked,
  onChange,
  label,
  prefix,
  compact = false,
  noBorder = false,
}: {
  name: string
  checked: boolean
  onChange: () => void
  label: string
  prefix?: ReactNode
  compact?: boolean
  noBorder?: boolean
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-center gap-3 px-5 py-3 transition-colors hover:bg-white/[0.02]",
        !compact && !noBorder && "border-b border-white/6",
        compact && "px-0 py-2",
      )}
    >
      <input
        type="radio"
        name={name}
        checked={checked}
        onChange={onChange}
        className="sr-only"
      />
      <span
        className={cn(
          "flex h-4 w-4 items-center justify-center rounded-full border-2 transition-colors",
          checked ? "border-primary" : "border-muted-foreground/45",
        )}
      >
        {checked && <span className="h-1.5 w-1.5 rounded-full bg-primary" />}
      </span>
      {prefix}
      <span className={cn("text-[13px]", checked ? "text-foreground" : "text-muted-foreground")}>
        {label}
      </span>
    </label>
  )
}

function HashcatStatusPanel({
  status,
}: {
  status: HashcatDetectionResult
}) {
  const { t } = useTranslation()

  return (
    <div className="af-panel-soft mt-3 space-y-2 p-3 text-xs">
      <div className={status.available ? "text-emerald-300" : "text-amber-300"}>
        {status.available
          ? t("hashcat_detected")
          : status.error ?? t("hashcat_not_detected")}
      </div>

      {status.version && (
        <div className="text-muted-foreground">
          {t("version")}: {status.version}
        </div>
      )}

      {status.path && (
        <div className="break-all font-mono text-[11px] text-muted-foreground">
          {status.path}
        </div>
      )}

      {status.devices.length > 0 && (
        <div className="space-y-1">
          <div className="text-[11px] text-muted-foreground">{t("hashcat_devices")}</div>
          <ul className="space-y-1 text-[11px] text-muted-foreground">
            {status.devices.map((device) => (
              <li key={`${device.id}-${device.name}`}>
                #{device.id} {device.name} ({device.device_type})
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

function StatBlock({
  label,
  value,
}: {
  label: string
  value: number
}) {
  return (
    <div>
      <div className="af-kicker mb-1">{label}</div>
      <div className="af-stat-number text-[22px] font-bold text-foreground">{value}</div>
    </div>
  )
}

function ConfirmDanger({
  text,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: {
  text: string
  confirmLabel: string
  cancelLabel: string
  onConfirm: () => void
  onCancel: () => void
}) {
  return (
    <div className="w-full rounded-xl border border-rose-400/18 bg-rose-400/8 p-4">
      <p className="text-sm text-rose-200">{text}</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <button onClick={onConfirm} className={`${DANGER_BUTTON_CLASS} px-3 py-2 text-xs`}>
          {confirmLabel}
        </button>
        <button onClick={onCancel} className={`${GHOST_BUTTON_CLASS} px-3 py-2 text-xs`}>
          {cancelLabel}
        </button>
      </div>
    </div>
  )
}

function AboutRow({
  label,
  value,
  noBorder = false,
}: {
  label: string
  value: ReactNode
  noBorder?: boolean
}) {
  return (
    <div
      className={cn(
        "flex items-start justify-between gap-3 px-5 py-3",
        !noBorder && "border-b border-white/6",
      )}
    >
      <span className="flex-shrink-0 text-sm text-muted-foreground">{label}</span>
      <span className="text-right text-sm font-medium text-foreground">{value}</span>
    </div>
  )
}
