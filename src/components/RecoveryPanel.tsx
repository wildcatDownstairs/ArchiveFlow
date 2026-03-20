import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import {
  Play,
  Square,
  KeyRound,
  Copy,
  Check,
  Zap,
  BookOpen,
  AlertCircle,
  FileUp,
} from "lucide-react"
import { open } from "@tauri-apps/plugin-dialog"
import { readTextFile } from "@tauri-apps/plugin-fs"
import { cn } from "@/lib/utils"
import { formatElapsed } from "@/lib/format"
import { buildDictionaryCandidates } from "@/lib/recoveryCandidates"
import { useAppStore } from "@/stores/appStore"
import * as api from "@/services/api"
import type { RecoveryProgress, RecoveryStatus, Task } from "@/types"

// 预定义字符集
const CHARSETS = {
  lowercase: "abcdefghijklmnopqrstuvwxyz",
  uppercase: "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
  digits: "0123456789",
  special: "!@#$%^&*()_+-=[]{}|;:',.<>?/~`\"\\",
}

type AttackTab = "dictionary" | "bruteforce" | "mask"

interface RecoveryPanelProps {
  task: Task
  onTaskStatusChange?: () => void
}

export default function RecoveryPanel({
  task,
  onTaskStatusChange,
}: RecoveryPanelProps) {
  const { t } = useTranslation()
  const recoveryPreferences = useAppStore((state) => state.recoveryPreferences)

  // 攻击模式
  const [activeTab, setActiveTab] = useState<AttackTab>("dictionary")

  // 字典模式配置
  const [wordlistText, setWordlistText] = useState("")
  const [dictionaryOptions, setDictionaryOptions] = useState({
    uppercase: false,
    capitalize: true,
    leetspeak: false,
    commonSuffixes: true,
    combineWords: false,
    includeFilenamePatterns: recoveryPreferences.autoIncludeFilenamePatterns,
  })

  // 暴力破解配置
  const [charsetFlags, setCharsetFlags] = useState(recoveryPreferences.defaultCharsetFlags)
  const [customCharset, setCustomCharset] = useState("")
  const [useCustomCharset, setUseCustomCharset] = useState(false)
  const [minLength, setMinLength] = useState(recoveryPreferences.defaultMinLength)
  const [maxLength, setMaxLength] = useState(recoveryPreferences.defaultMaxLength)
  const [maskPattern, setMaskPattern] = useState("?d?d?d?d")

  // 恢复状态
  const [progress, setProgress] = useState<RecoveryProgress | null>(null)
  const [isRunning, setIsRunning] = useState(
    task.status === "processing",
  )
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  // unlisten ref
  const unlistenRef = useRef<(() => void) | null>(null)

  // 监听恢复进度
  useEffect(() => {
    if (!isRunning) return

    let cancelled = false

    api.onRecoveryProgress((p) => {
      if (cancelled || p.task_id !== task.id) return
      setProgress(p)

      // 终态处理
      if (p.status !== "running") {
        setIsRunning(false)
        onTaskStatusChange?.()
      }
    }).then((unlisten) => {
      if (cancelled) {
        unlisten()
      } else {
        unlistenRef.current = unlisten
      }
    })

    return () => {
      cancelled = true
      unlistenRef.current?.()
      unlistenRef.current = null
    }
  }, [isRunning, task.id, onTaskStatusChange])

  // 构建字符集
  const buildCharset = useCallback(() => {
    if (useCustomCharset) return customCharset
    let charset = ""
    if (charsetFlags.lowercase) charset += CHARSETS.lowercase
    if (charsetFlags.uppercase) charset += CHARSETS.uppercase
    if (charsetFlags.digits) charset += CHARSETS.digits
    if (charsetFlags.special) charset += CHARSETS.special
    return charset
  }, [charsetFlags, customCharset, useCustomCharset])

  const handleImportDictionaryFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Text", extensions: ["txt", "dic", "lst"] }],
      })
      if (!selected || Array.isArray(selected)) return

      const content = await readTextFile(selected)
      setWordlistText((prev) => [prev, content].filter(Boolean).join(prev ? "\n" : ""))
    } catch (e) {
      setError(String(e))
    }
  }

  // 开始恢复
  const handleStart = async () => {
    setError(null)
    setProgress(null)

    try {
      if (activeTab === "dictionary") {
        const lines = wordlistText
          .split("\n")
          .map((l) => l.trim())
          .filter(Boolean)
        const candidates = buildDictionaryCandidates(
          lines,
          task.file_name,
          dictionaryOptions,
        )
        if (candidates.length === 0) {
          setError(t("dictionary_empty"))
          return
        }
        await api.startRecovery(
          task.id,
          "dictionary",
          JSON.stringify({ wordlist: candidates }),
        )
        if (recoveryPreferences.autoClearDictionaryInput) {
          setWordlistText("")
        }
      } else if (activeTab === "bruteforce") {
        const charset = buildCharset()
        if (charset.length === 0) {
          setError(t("charset_empty"))
          return
        }
        if (maxLength < minLength) {
          setError(t("invalid_length"))
          return
        }
        await api.startRecovery(
          task.id,
          "bruteforce",
          JSON.stringify({
            charset,
            min_length: minLength,
            max_length: maxLength,
          }),
        )
      } else {
        if (!maskPattern.trim()) {
          setError(t("mask_empty"))
          return
        }
        await api.startRecovery(
          task.id,
          "mask",
          JSON.stringify({
            mask: maskPattern.trim(),
          }),
        )
      }
      setIsRunning(true)
      onTaskStatusChange?.()
    } catch (e) {
      setError(String(e))
    }
  }

  // 取消恢复
  const handleCancel = async () => {
    try {
      await api.cancelRecovery(task.id)
    } catch (e) {
      setError(String(e))
    }
  }

  // 复制密码
  const handleCopy = async (password: string) => {
    try {
      await navigator.clipboard.writeText(password)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // fallback: noop
    }
  }

  // 进度百分比
  const progressPercent =
    progress && progress.total > 0
      ? Math.min((progress.tried / progress.total) * 100, 100)
      : 0

  const terminalStatus:
    | Exclude<RecoveryStatus, "running">
    | "interrupted"
    | null =
    progress && progress.status !== "running"
      ? progress.status
      : task.status === "succeeded"
        ? "found"
        : task.status === "exhausted"
          ? "exhausted"
          : task.status === "cancelled"
            ? "cancelled"
            : task.status === "interrupted"
              ? "interrupted"
            : task.status === "failed"
              ? "error"
              : null

  const displayPassword =
    progress?.status === "found" ? progress.found_password : task.found_password
  const visiblePassword =
    displayPassword && recoveryPreferences.resultRetentionPolicy === "masked"
      ? "•".repeat(displayPassword.length)
      : displayPassword
  const terminalErrorMessage =
    terminalStatus === "error" || terminalStatus === "interrupted"
      ? task.error_message
      : null

  // 状态显示
  const statusDisplay: Record<
    Exclude<RecoveryStatus, "running"> | "interrupted",
    { color: string; icon: typeof Check; label: string }
  > = {
    found: { color: "text-green-600", icon: Check, label: t("password_found") },
    exhausted: {
      color: "text-yellow-600",
      icon: AlertCircle,
      label: t("recovery_exhausted"),
    },
    cancelled: {
      color: "text-gray-600",
      icon: Square,
      label: t("recovery_cancelled"),
    },
    error: {
      color: "text-red-600",
      icon: AlertCircle,
      label: t("recovery_error"),
    },
    interrupted: {
      color: "text-orange-600",
      icon: AlertCircle,
      label: t("recovery_interrupted"),
    },
  }

  // 只有加密且状态允许时才显示
  const canStart =
    !isRunning &&
    (
      task.status === "ready" ||
      task.status === "failed" ||
      task.status === "cancelled" ||
      task.status === "exhausted" ||
      task.status === "interrupted"
    )

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2">
        <KeyRound className="h-5 w-5 text-amber-500" />
        <h2 className="text-lg font-semibold">{t("recovery")}</h2>
      </div>

      <p className="text-sm text-muted-foreground">
        {t("recovery_description")}
      </p>

      {/* 结果展示 - 如果找到密码 */}
      {terminalStatus === "found" && displayPassword && (
        <div className="rounded-lg border-2 border-green-300 bg-green-50 p-4 space-y-3">
          <div className="flex items-center gap-2 text-green-700 font-medium">
            <Check className="h-5 w-5" />
            {t("password_found")}
          </div>
          <div className="flex flex-col gap-3 md:flex-row md:items-start">
            <code
              className="flex-1 rounded border border-green-200 bg-white px-3 py-2 font-mono text-base leading-relaxed break-all text-slate-950 select-all shadow-sm"
              title={displayPassword}
            >
              {visiblePassword}
            </code>
            <button
              onClick={() => void handleCopy(displayPassword)}
              className="inline-flex items-center justify-center gap-1.5 rounded-md px-3 py-2 text-sm bg-green-600 text-white hover:bg-green-700 transition-colors md:self-stretch"
            >
              {copied ? (
                <Check className="h-4 w-4" />
              ) : (
                <Copy className="h-4 w-4" />
              )}
              {copied ? t("copied") : t("copy_password")}
            </button>
          </div>
          {progress?.status === "found" && (
            <div className="text-xs text-green-600">
              {t("tried_count")}: {progress.tried.toLocaleString()} |{" "}
              {t("elapsed_time")}: {formatElapsed(progress.elapsed_seconds)}
            </div>
          )}
        </div>
      )}

      {/* 结果展示 - 终态（非 found） */}
      {terminalStatus &&
        terminalStatus !== "found" && (
          <div
            className={cn(
              "rounded-lg border p-4 flex items-center gap-3",
              terminalStatus === "exhausted" &&
                "border-yellow-200 bg-yellow-50",
              terminalStatus === "cancelled" && "border-gray-200 bg-gray-50",
              terminalStatus === "interrupted" &&
                "border-orange-200 bg-orange-50",
              terminalStatus === "error" && "border-red-200 bg-red-50",
            )}
          >
            {(() => {
              const info = statusDisplay[terminalStatus]
              const Icon = info.icon
              return (
                <>
                  <Icon className={cn("h-5 w-5", info.color)} />
                  <div className="flex flex-col gap-1">
                    <span className={cn("font-medium", info.color)}>
                      {info.label}
                    </span>
                    {terminalErrorMessage && (
                      <span className="text-sm text-muted-foreground">
                        {terminalErrorMessage}
                      </span>
                    )}
                  </div>
                  {progress && progress.status !== "running" && (
                    <span className="text-sm text-muted-foreground ml-auto">
                      {t("tried_count")}: {progress.tried.toLocaleString()} |{" "}
                      {t("elapsed_time")}:{" "}
                      {formatElapsed(progress.elapsed_seconds)}
                    </span>
                  )}
                </>
              )
            })()}
          </div>
        )}

      {/* 进度条 - 运行中 */}
      {isRunning && progress && (
        <div className="rounded-lg border p-4 space-y-3">
          <div className="flex items-center justify-between text-sm">
            <span className="font-medium text-indigo-600">
              {t("recovery_running")}
            </span>
            <span className="text-muted-foreground">
              {progressPercent.toFixed(1)}%
            </span>
          </div>

          {/* 进度条 */}
          <div className="h-2.5 rounded-full bg-gray-200 overflow-hidden">
            <div
              className="h-full rounded-full bg-indigo-500 transition-all duration-300"
              style={{ width: `${progressPercent}%` }}
            />
          </div>

          {/* 统计数据 */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-sm">
            <div>
              <span className="text-muted-foreground">{t("tried_count")}</span>
              <p className="font-mono font-medium">
                {progress.tried.toLocaleString()}
              </p>
            </div>
            <div>
              <span className="text-muted-foreground">{t("total_count")}</span>
              <p className="font-mono font-medium">
                {progress.total.toLocaleString()}
              </p>
            </div>
            <div>
              <span className="text-muted-foreground">{t("speed")}</span>
              <p className="font-mono font-medium">
                {progress.speed.toFixed(0)} {t("passwords_per_sec")}
              </p>
            </div>
            <div>
              <span className="text-muted-foreground">
                {t("elapsed_time")}
              </span>
              <p className="font-mono font-medium">
                {formatElapsed(progress.elapsed_seconds)}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* 配置面板 - 非运行状态且可以开始 */}
      {canStart && (
        <div className="rounded-lg border p-4 space-y-4">
          {/* 模式选择 Tab */}
          <div className="flex border-b">
            <button
              onClick={() => setActiveTab("dictionary")}
              className={cn(
                "flex items-center gap-2 px-4 py-2 text-sm font-medium border-b-2 transition-colors",
                activeTab === "dictionary"
                  ? "border-indigo-500 text-indigo-600"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )}
            >
              <BookOpen className="h-4 w-4" />
              {t("dictionary_attack")}
            </button>
            <button
              onClick={() => setActiveTab("bruteforce")}
              className={cn(
                "flex items-center gap-2 px-4 py-2 text-sm font-medium border-b-2 transition-colors",
                activeTab === "bruteforce"
                  ? "border-indigo-500 text-indigo-600"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )}
            >
              <Zap className="h-4 w-4" />
              {t("bruteforce_attack")}
            </button>
            <button
              onClick={() => setActiveTab("mask")}
              className={cn(
                "flex items-center gap-2 px-4 py-2 text-sm font-medium border-b-2 transition-colors",
                activeTab === "mask"
                  ? "border-indigo-500 text-indigo-600"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )}
            >
              <KeyRound className="h-4 w-4" />
              {t("mask_attack")}
            </button>
          </div>

          {/* 字典模式 */}
          {activeTab === "dictionary" && (
            <div className="space-y-4">
              <div className="flex items-center justify-between gap-3">
                <label className="text-sm text-muted-foreground">
                  {t("dictionary_hint")}
                </label>
                <button
                  onClick={() => void handleImportDictionaryFile()}
                  className="inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
                >
                  <FileUp className="h-3.5 w-3.5" />
                  {t("import_dictionary_file")}
                </button>
              </div>
              <label className="text-sm text-muted-foreground">
                {t("dictionary_generation_hint")}
              </label>
              <textarea
                value={wordlistText}
                onChange={(e) => setWordlistText(e.target.value)}
                placeholder={t("dictionary_placeholder")}
                rows={6}
                className="w-full rounded-md border bg-background px-3 py-2 text-sm font-mono resize-y focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <p className="text-xs text-muted-foreground">
                {
                  wordlistText
                    .split("\n")
                    .map((l) => l.trim())
                    .filter(Boolean).length
                }{" "}
                {t("items")}
              </p>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-sm">
                {(
                  [
                    ["capitalize", t("transform_capitalize")],
                    ["uppercase", t("transform_uppercase")],
                    ["leetspeak", t("transform_leetspeak")],
                    ["commonSuffixes", t("transform_common_suffixes")],
                    ["combineWords", t("combine_dictionary")],
                    ["includeFilenamePatterns", t("include_filename_patterns")],
                  ] as const
                ).map(([key, label]) => (
                  <label key={key} className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={dictionaryOptions[key]}
                      onChange={(e) =>
                        setDictionaryOptions((prev) => ({
                          ...prev,
                          [key]: e.target.checked,
                        }))
                      }
                      className="rounded border-gray-300"
                    />
                    {label}
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* 暴力破解模式 */}
          {activeTab === "bruteforce" && (
            <div className="space-y-4">
              {/* 字符集选择 */}
              <div className="space-y-2">
                <label className="text-sm font-medium">{t("charset")}</label>
                <div className="grid grid-cols-2 gap-2">
                  {(
                    [
                      "lowercase",
                      "uppercase",
                      "digits",
                      "special",
                    ] as const
                  ).map((key) => (
                    <label
                      key={key}
                      className="flex items-center gap-2 text-sm cursor-pointer"
                    >
                      <input
                        type="checkbox"
                        checked={!useCustomCharset && charsetFlags[key]}
                        onChange={(e) => {
                          setUseCustomCharset(false)
                          setCharsetFlags((prev) => ({
                            ...prev,
                            [key]: e.target.checked,
                          }))
                        }}
                        className="rounded border-gray-300"
                      />
                      {t(`charset_${key}`)}
                    </label>
                  ))}
                  <label className="flex items-center gap-2 text-sm cursor-pointer col-span-2">
                    <input
                      type="checkbox"
                      checked={useCustomCharset}
                      onChange={(e) => setUseCustomCharset(e.target.checked)}
                      className="rounded border-gray-300"
                    />
                    {t("charset_custom")}
                  </label>
                </div>
                {useCustomCharset && (
                  <input
                    type="text"
                    value={customCharset}
                    onChange={(e) => setCustomCharset(e.target.value)}
                    placeholder={t("charset_custom_placeholder")}
                    className="w-full rounded-md border bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500"
                  />
                )}
                {/* 当前字符集预览 */}
                <p className="text-xs text-muted-foreground font-mono truncate">
                  {buildCharset().slice(0, 60)}
                  {buildCharset().length > 60 ? "..." : ""}
                  {" "}({buildCharset().length} chars)
                </p>
              </div>

              {/* 长度设置 */}
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <label className="text-sm font-medium">
                    {t("min_length")}
                  </label>
                  <input
                    type="number"
                    value={minLength}
                    onChange={(e) =>
                      setMinLength(Math.max(1, parseInt(e.target.value) || 1))
                    }
                    min={1}
                    max={12}
                    className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-sm font-medium">
                    {t("max_length")}
                  </label>
                  <input
                    type="number"
                    value={maxLength}
                    onChange={(e) =>
                      setMaxLength(Math.max(1, parseInt(e.target.value) || 1))
                    }
                    min={1}
                    max={12}
                    className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                  />
                </div>
              </div>
            </div>
          )}

          {activeTab === "mask" && (
            <div className="space-y-3">
              <div className="space-y-2">
                <label className="text-sm font-medium">{t("mask_pattern")}</label>
                <input
                  type="text"
                  value={maskPattern}
                  onChange={(e) => setMaskPattern(e.target.value)}
                  placeholder={t("mask_placeholder")}
                  className="w-full rounded-md border bg-background px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
                <p className="text-xs text-muted-foreground">
                  {t("mask_hint")}
                </p>
              </div>
            </div>
          )}

          {/* 错误提示 */}
          {error && (
            <div className="rounded-md bg-red-50 border border-red-200 p-3 text-red-700 text-sm flex items-center gap-2">
              <AlertCircle className="h-4 w-4 flex-shrink-0" />
              {error}
            </div>
          )}

          {/* 操作按钮 */}
          <button
            onClick={() => void handleStart()}
            className="flex items-center gap-2 rounded-md px-4 py-2 text-sm font-medium bg-indigo-600 text-white hover:bg-indigo-700 transition-colors"
          >
            <Play className="h-4 w-4" />
            {t("start_recovery")}
          </button>
        </div>
      )}

      {/* 取消按钮 - 运行中 */}
      {isRunning && (
        <button
          onClick={() => void handleCancel()}
          className="flex items-center gap-2 rounded-md px-4 py-2 text-sm font-medium bg-red-600 text-white hover:bg-red-700 transition-colors"
        >
          <Square className="h-4 w-4" />
          {t("cancel_recovery")}
        </button>
      )}
    </section>
  )
}
