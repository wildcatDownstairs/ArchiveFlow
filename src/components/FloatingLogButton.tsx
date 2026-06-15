import { AlertCircle, FolderOpen, Loader2 } from "lucide-react"
import { useCallback, useState } from "react"
import * as api from "@/services/api"

function formatError(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}

export default function FloatingLogButton() {
  const [opening, setOpening] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)

  const handleOpenLogs = useCallback(async () => {
    if (opening) return

    setOpening(true)
    setErrorMessage(null)
    api.recordAppLog("INFO", "user", "用户点击打开日志文件夹")

    try {
      await api.openLogDir()
    } catch (error) {
      const message = formatError(error)
      setErrorMessage(message)
      api.recordAppLog("ERROR", "error", `打开日志文件夹失败: ${message}`)
    } finally {
      setOpening(false)
    }
  }, [opening])

  return (
    <div className="af-log-fab-wrap">
      {errorMessage && (
        <div className="af-log-fab-status" role="status">
          <AlertCircle size={14} aria-hidden="true" />
          <span>日志目录打开失败：{errorMessage}</span>
        </div>
      )}
      <button
        type="button"
        className="af-log-fab"
        onClick={handleOpenLogs}
        disabled={opening}
        title="打开日志文件夹"
        aria-label="打开日志文件夹"
      >
        {opening ? (
          <Loader2 className="af-log-fab-spinner" size={18} aria-hidden="true" />
        ) : (
          <FolderOpen size={18} aria-hidden="true" />
        )}
        <span>日志</span>
      </button>
    </div>
  )
}
