# Cyberpunk Boot Screen Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a full-screen cyberpunk/glitch-art boot overlay that plays through 6 labelled startup steps before fading out to the main app UI, eliminating the blank white screen on startup.

**Architecture:** A `BootScreen` component renders over the whole app from mount, runs 6 sequential async steps (i18n ready → backend ping → tasks → audit events → prefs → done), updates a step-index state at each completion, then calls `onComplete()` which unmounts it. All animations are pure CSS `@keyframes` — no new npm dependencies.

**Tech Stack:** React 19, Tailwind v4, TypeScript, Zustand, `@tauri-apps/api`, `react-i18next`, `lucide-react`

---

## Task 1: Add CSS animations to index.css

**Files:**
- Modify: `src/index.css`

**Step 1: Add the four `@keyframes` blocks and a `prefers-reduced-motion` override**

Append the following to the bottom of `src/index.css`:

```css
/* ===== Boot screen animations ===== */

/* Continuous glitch displacement on the logo text */
@keyframes glitch-shift {
  0%   { clip-path: inset(40% 0 61% 0); transform: translateX(-4px); text-shadow: 3px 0 #ff00c8, -3px 0 #00fff7; }
  10%  { clip-path: inset(92% 0 1% 0);  transform: translateX(4px);  text-shadow: -3px 0 #ff00c8, 3px 0 #00fff7; }
  20%  { clip-path: inset(43% 0 1% 0);  transform: translateX(0);    text-shadow: 3px 0 #ff00c8, -3px 0 #00fff7; }
  30%  { clip-path: inset(25% 0 58% 0); transform: translateX(3px);  text-shadow: -2px 0 #ff00c8, 2px 0 #00fff7; }
  40%  { clip-path: inset(54% 0 7% 0);  transform: translateX(-3px); text-shadow: 2px 0 #ff00c8, -2px 0 #00fff7; }
  50%  { clip-path: inset(58% 0 43% 0); transform: translateX(4px);  text-shadow: -3px 0 #ff00c8, 3px 0 #00fff7; }
  60%  { clip-path: inset(50% 0 1% 0);  transform: translateX(-4px); text-shadow: 3px 0 #ff00c8, -3px 0 #00fff7; }
  70%  { clip-path: inset(67% 0 27% 0); transform: translateX(0);    text-shadow: -2px 0 #ff00c8, 2px 0 #00fff7; }
  80%  { clip-path: inset(14% 0 73% 0); transform: translateX(-4px); text-shadow: 3px 0 #ff00c8, -3px 0 #00fff7; }
  90%  { clip-path: inset(97% 0 2% 0);  transform: translateX(4px);  text-shadow: -3px 0 #ff00c8, 3px 0 #00fff7; }
  100% { clip-path: inset(40% 0 61% 0); transform: translateX(-4px); text-shadow: 3px 0 #ff00c8, -3px 0 #00fff7; }
}

/* Blinking underscore cursor on the active step */
@keyframes blink-cursor {
  0%, 100% { opacity: 1; }
  50%       { opacity: 0; }
}

/* Slow drift of the CRT scanline overlay */
@keyframes scanline-drift {
  from { background-position: 0 0; }
  to   { background-position: 0 100%; }
}

/* Shimmer sweep across the progress bar fill */
@keyframes progress-shine {
  from { background-position: -200% center; }
  to   { background-position: 200% center; }
}

/* Boot overlay fade-out */
@keyframes boot-fadeout {
  from { opacity: 1; }
  to   { opacity: 0; }
}

/* Disable motion-heavy animations for users who prefer reduced motion */
@media (prefers-reduced-motion: reduce) {
  .boot-glitch-layer { animation: none !important; }
  .boot-cursor       { animation: none !important; opacity: 1 !important; }
  .boot-scanlines    { animation: none !important; }
  .boot-progress-fill { animation: none !important; }
}
```

**Step 2: Verify the CSS file still compiles (no Tailwind errors)**

Run: `npm run build 2>&1 | head -20`  
Expected: no errors about `index.css`

**Step 3: Commit**

```bash
git add src/index.css
git commit -m "feat: add boot screen CSS keyframes"
```

---

## Task 2: Add i18n boot-step strings

**Files:**
- Modify: `src/i18n/index.ts`

**Step 1: Add boot step keys to the `zh` translation block**

Find the `zh.translation` object. Add these entries (e.g. after the `loading` key):

```ts
boot_step_1: "初始化运行时环境",
boot_step_2: "建立数据库连接",
boot_step_3: "加载任务列表",
boot_step_4: "读取审计记录",
boot_step_5: "同步应用配置",
boot_step_6: "系统就绪",
```

**Step 2: Add the same keys to the `en` translation block**

```ts
boot_step_1: "Initializing runtime",
boot_step_2: "Connecting to database",
boot_step_3: "Loading task list",
boot_step_4: "Reading audit records",
boot_step_5: "Syncing app config",
boot_step_6: "System ready",
```

**Step 3: Verify TypeScript compiles**

Run: `npm run build 2>&1 | tail -5`  
Expected: no errors

**Step 4: Commit**

```bash
git add src/i18n/index.ts
git commit -m "feat: add boot screen i18n strings"
```

---

## Task 3: Create BootScreen component

**Files:**
- Create: `src/components/BootScreen.tsx`

**Step 1: Write the component**

Create `src/components/BootScreen.tsx` with this exact content:

```tsx
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
      () => api.getAuditEvents({ limit: 50 }).then(() => undefined).catch(() => undefined),
      // Step 5: localStorage prefs are read synchronously at store init; small delay
      () => new Promise((r) => setTimeout(r, 80)),
      // Step 6: final "ready" pause before fade-out
      () => new Promise((r) => setTimeout(r, 400)),
    ]

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
      <div style={{ position: "relative", width: "min(520px, 90vw)" }}>
        {/* Logo with glitch effect — two pseudo-layers via sibling divs */}
        <div style={{ position: "relative", marginBottom: "3rem", textAlign: "center" }}>
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
        <div style={{ marginBottom: "1.75rem", display: "flex", flexDirection: "column", gap: "0.45rem" }}>
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
                  fontSize: "0.8rem",
                  opacity: isPending ? 0.3 : isDone ? 0.5 : 1,
                  transition: "opacity 0.3s",
                  color: isDone ? "#555" : isActive ? "#00fff7" : "#444",
                }}
              >
                <span style={{ color: isActive ? "#00fff7" : isDone ? "#333" : "#2a2a2a", width: "1.2rem" }}>
                  {isDone ? "✓" : isActive ? "›" : "·"}
                </span>
                <span style={{ flex: 1 }}>{t(key)}</span>
                {isDone && (
                  <span style={{ color: "#ff00c8", fontSize: "0.7rem", letterSpacing: "0.05em" }}>
                    [OK]
                  </span>
                )}
                {isActive && (
                  <span
                    className="boot-cursor"
                    style={{
                      color: "#00fff7",
                      fontSize: "0.8rem",
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
              height: "2px",
              backgroundColor: "#0a0a0a",
              borderRadius: "1px",
              overflow: "hidden",
              border: "1px solid #111",
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
                borderRadius: "1px",
              }}
            />
          </div>
          <div
            style={{
              textAlign: "right",
              marginTop: "0.3rem",
              fontSize: "0.65rem",
              color: "#00fff7",
              opacity: 0.6,
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
            fontSize: "0.6rem",
            color: "#222",
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
```

**Step 2: Check TypeScript compiles cleanly**

Run: `npm run build 2>&1 | tail -10`  
Expected: no TS errors

**Step 3: Commit**

```bash
git add src/components/BootScreen.tsx
git commit -m "feat: add BootScreen cyberpunk component"
```

---

## Task 4: Wire BootScreen into App.tsx

**Files:**
- Modify: `src/App.tsx`

**Step 1: Replace App.tsx with the wired version**

```tsx
import { useState } from "react"
import AppRouter from "@/router"
import BootScreen from "@/components/BootScreen"

function App() {
  const [isBooting, setIsBooting] = useState(true)

  if (isBooting) {
    return <BootScreen onComplete={() => setIsBooting(false)} />
  }

  return <AppRouter />
}

export default App
```

**Step 2: Verify build**

Run: `npm run build 2>&1 | tail -10`  
Expected: clean build, no errors

**Step 3: Run frontend tests**

Run: `npm run test:run 2>&1 | tail -20`  
Expected: all tests pass (BootScreen is not imported in any existing test)

**Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "feat: show BootScreen on startup, replace blank white screen"
```

---

## Task 5: Fix the api.getAuditEvents call signature

**Files:**
- Read: `src/services/api.ts` — check exact signature of `getAuditEvents`

**Step 1: Inspect current signature**

Open `src/services/api.ts` and find the `getAuditEvents` function.  
It likely accepts `limit?: number` directly, not `{ limit }`.

**Step 2: Fix the call in BootScreen.tsx if needed**

If `getAuditEvents` signature is `getAuditEvents(limit?: number)`, update the step 4 runner in `BootScreen.tsx`:

```ts
// Change:
() => api.getAuditEvents({ limit: 50 }).then(() => undefined).catch(() => undefined),
// To:
() => api.getAuditEvents(50).then(() => undefined).catch(() => undefined),
```

If it already accepts an object `{ limit }`, leave it as-is.

**Step 3: Verify build and tests**

Run: `npm run build 2>&1 | tail -5`  
Run: `npm run test:run 2>&1 | tail -10`  
Expected: no errors

**Step 4: Commit if changed**

```bash
git add src/components/BootScreen.tsx
git commit -m "fix: correct getAuditEvents call signature in BootScreen"
```

---

## Task 6: Manual smoke test

**Goal:** Visually verify the boot screen looks correct before calling it done.

**Step 1: Start dev server**

Run: `npm run tauri dev`

**Step 2: Observe boot sequence**

- [ ] Black screen appears immediately (no white flash)
- [ ] `ARCHIVEFLOW` logo with cyan colour and glitch/RGB-split animation
- [ ] 6 steps listed, each activating in sequence with `›` and blinking `_`
- [ ] Completed steps show `✓` and dimmed text with `[OK]` in magenta
- [ ] Progress bar advances with each step, shows percentage
- [ ] Footer text `SYS_BOOT v0.1.0 // ARCHIVEFLOW` visible at bottom
- [ ] After step 6, screen fades out smoothly to main app UI
- [ ] Main app loads with tasks already in the store (no second loading flash)

**Step 3: Verify reduced-motion (optional)**

In Windows: Settings → Ease of Access → Display → Show animations → Off  
Expected: logo is static (no glitch), cursor is steady, progress bar has no shimmer — steps and bar still advance normally

---

## Task 7: Final commit

```bash
git add -A
git commit -m "feat: cyberpunk boot screen replaces blank startup white screen"
```
