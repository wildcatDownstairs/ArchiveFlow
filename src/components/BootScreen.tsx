/**
 * @fileoverview 文件功能：实现 BootScreen UI 组件
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies react, react-i18next
 */

import { useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import * as api from "@/services/api"
import { useTaskStore } from "@/stores/taskStore"

interface Props {
  onComplete: () => void
}

const TOTAL_STEPS = 6

// Each step: label key + the async work to perform
// Steps run sequentially; each resolves before the next begins.
type StepRunner = () => Promise<void>

/**
 *
 * @param root0
 * @param root0.onComplete
  * @returns {JSX.Element} 渲染的 React 元素
 */
export default function BootScreen({ onComplete }: Props) {
  const { t } = useTranslation()
  const fetchTasks = useTaskStore((s) => s.fetchTasks)

  // currentStep: 0 = nothing started, 1–6 = step N in progress, 7 = all done
  const [currentStep, setCurrentStep] = useState(0)
  const [fadingOut, setFadingOut] = useState(false)
  const ran = useRef(false) // prevent double-run in StrictMode

  const stepKeys = [
    "boot_step_1",
    "boot_step_2",
    "boot_step_3",
    "boot_step_4",
    "boot_step_5",
    "boot_step_6",
  ] as const

  // Hide the static HTML splash only after React has rendered its first active step,
  // preventing a flash of the blank currentStep=0 state between the two screens.
  useEffect(() => {
    if (currentStep < 1) return
    const splash = document.getElementById("static-splash")
    if (splash) splash.classList.add("hidden")
  }, [currentStep])

  useEffect(() => {
    if (ran.current) return
    ran.current = true

    const steps: StepRunner[] = [
      // Step 1: i18n is already initialised synchronously; small delay for visual effect
      () => new Promise((r) => setTimeout(r, 180)),
      // Step 2: ping backend via getStats
      () => api.getStats().then(() => undefined).catch(() => undefined),
      // Step 3: load tasks into store
      () => fetchTasks(),
      // Step 4: prefetch recent audit events (fire-and-forget on error)
      () => api.getAuditEvents(50).then(() => undefined).catch(() => undefined),
      // Step 5: localStorage prefs are read synchronously at store init; small delay
      () => new Promise((r) => setTimeout(r, 80)),
      // Step 6: final "ready" pause before fade-out
      () => new Promise((r) => setTimeout(r, 400)),
    ]

    /**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
    async function run() {
      for (let i = 0; i < steps.length; i++) {
        setCurrentStep(i + 1)
        await steps[i]()
      }
      setCurrentStep(TOTAL_STEPS + 1) // all done
      setFadingOut(true)
      // wait for fade-out animation (600ms) then unmount
      setTimeout(onComplete, 600)
    }

    void run()
  }, [fetchTasks, onComplete])

  const progressPct = Math.min(
    Math.round((Math.max(currentStep - 1, 0) / TOTAL_STEPS) * 100),
    100,
  )

  return (
    <div
      className="boot-overlay"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        backgroundColor: "#000",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: "'Cascadia Code', 'Source Code Pro', ui-monospace, monospace",
        animation: fadingOut ? "boot-fadeout 0.6s ease-out forwards" : undefined,
      }}
    >
      {/* CRT scanline overlay */}
      <div
        className="boot-scanlines"
        style={{
          position: "absolute",
          inset: 0,
          pointerEvents: "none",
          backgroundImage:
            "repeating-linear-gradient(0deg, transparent, transparent 2px, rgba(0,255,247,0.03) 2px, rgba(0,255,247,0.03) 4px)",
          backgroundSize: "100% 4px",
          animation: "scanline-drift 8s linear infinite",
        }}
      />

      {/* Content wrapper */}
      <div style={{ position: "relative", width: "min(560px, 90vw)" }}>
        {/* Logo with glitch effect — two pseudo-layers via sibling divs */}
        <div style={{ position: "relative", marginBottom: "2.5rem", textAlign: "center" }}>
          {/* Base layer */}
          <div
            style={{
              fontSize: "clamp(1.8rem, 5vw, 2.8rem)",
              fontWeight: 900,
              letterSpacing: "0.25em",
              color: "#00fff7",
              textShadow: "0 0 20px rgba(0,255,247,0.5)",
            }}
          >
            ARCHIVEFLOW
          </div>
          {/* Glitch layer 1 (magenta) */}
          <div
            className="boot-glitch-layer"
            aria-hidden="true"
            style={{
              position: "absolute",
              inset: 0,
              fontSize: "clamp(1.8rem, 5vw, 2.8rem)",
              fontWeight: 900,
              letterSpacing: "0.25em",
              color: "#ff00c8",
              textAlign: "center",
              animation: "glitch-shift 2.4s infinite linear alternate-reverse",
              mixBlendMode: "screen",
              userSelect: "none",
            }}
          >
            ARCHIVEFLOW
          </div>
          {/* Glitch layer 2 (cyan, offset timing) */}
          <div
            className="boot-glitch-layer"
            aria-hidden="true"
            style={{
              position: "absolute",
              inset: 0,
              fontSize: "clamp(1.8rem, 5vw, 2.8rem)",
              fontWeight: 900,
              letterSpacing: "0.25em",
              color: "#00fff7",
              textAlign: "center",
              animation: "glitch-shift 3.1s 0.7s infinite linear alternate",
              mixBlendMode: "screen",
              userSelect: "none",
            }}
          >
            ARCHIVEFLOW
          </div>
        </div>

        {/* Step list */}
        <div style={{ marginBottom: "1.75rem", display: "flex", flexDirection: "column", gap: "0.55rem" }}>
          {stepKeys.map((key, idx) => {
            const stepNum = idx + 1
            const isDone = currentStep > stepNum
            const isActive = currentStep === stepNum
            const isPending = currentStep < stepNum

            return (
              <div
                key={key}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.6rem",
                  fontSize: "1.05rem",
                  lineHeight: "1.6",
                  opacity: isPending ? 0.55 : isDone ? 0.75 : 1,
                  transition: "opacity 0.3s",
                  color: isDone ? "#38ccc4" : isActive ? "#00fff7" : "#8899aa",
                  ...(isDone ? {
                    textShadow: "-1px 0 rgba(255,0,200,0.4), 1px 0 rgba(0,255,247,0.4)",
                    animation: "done-glitch 5.5s ease-in-out infinite",
                    animationDelay: `${idx * 0.65}s`,
                  } : isPending ? {
                    textShadow: "-1px 0 rgba(255,0,200,0.5), 1px 0 rgba(0,255,247,0.5)",
                    animation: "pending-glitch 7s ease-in-out infinite",
                  } : {}),
                }}
              >
                <span style={{ color: isActive ? "#00fff7" : isDone ? "#28a89f" : "#667788", width: "1.2rem", lineHeight: "inherit" }}>
                  {isDone ? "✓" : isActive ? "›" : "·"}
                </span>
                <span style={{ flex: 1 }}>{t(key)}</span>
                {isDone && (
                  <span style={{ color: "#ff00c8", fontSize: "0.85rem", letterSpacing: "0.05em" }}>
                    [OK]
                  </span>
                )}
                {isActive && (
                  <span
                    className="boot-cursor"
                    style={{
                      color: "#00fff7",
                      fontSize: "0.95rem",
                      animation: "blink-cursor 0.7s step-end infinite",
                    }}
                  >
                    _
                  </span>
                )}
              </div>
            )
          })}
        </div>

        {/* Progress bar */}
        <div style={{ marginBottom: "1rem" }}>
          <div
            style={{
              height: "6px",
              backgroundColor: "#0a0a0a",
              borderRadius: "3px",
              overflow: "hidden",
              border: "1px solid #1a1a1a",
            }}
          >
            <div
              className="boot-progress-fill"
              style={{
                height: "100%",
                width: `${progressPct}%`,
                background:
                  "linear-gradient(90deg, #004d4d, #00fff7 40%, #80ffff 50%, #00fff7 60%, #004d4d)",
                backgroundSize: "200% 100%",
                animation: "progress-shine 1.5s linear infinite",
                transition: "width 0.35s ease-out",
                borderRadius: "2px",
              }}
            />
          </div>
          <div
            style={{
              textAlign: "right",
              marginTop: "0.35rem",
              fontSize: "0.85rem",
              color: "#00fff7",
              opacity: 0.8,
              letterSpacing: "0.05em",
            }}
          >
            {progressPct}%
          </div>
        </div>

        {/* Footer */}
        <div
          style={{
            textAlign: "center",
            fontSize: "0.7rem",
            color: "#444",
            letterSpacing: "0.12em",
            textTransform: "uppercase",
            marginTop: "0.5rem",
          }}
        >
          SYS_BOOT v0.1.0 // ARCHIVEFLOW
        </div>
      </div>
    </div>
  )
}
