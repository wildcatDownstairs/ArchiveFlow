# Cyberpunk Boot Screen Design

**Date:** 2026-03-21  
**Status:** Approved

## Problem

On startup, the app shows a blank white screen for several seconds while JS loads, i18n initialises, and the initial `fetchTasks()` call completes. This is jarring UX with no feedback.

## Solution

A full-screen boot overlay that displays immediately on mount, walks through 6 labelled startup steps with Glitch art / cyberpunk aesthetics, then fades out once all data is ready.

## Visual Design

### Palette
- Background: pure black `#000000`
- Primary accent: cyan `#00fff7`
- Secondary accent: magenta `#ff00c8`
- Text: cyan for active step, magenta for completed `[OK]`, dim gray for pending steps
- Progress bar: cyan fill with scan-light animation

### Elements (top to bottom)
1. **CRT scanline overlay** — `repeating-linear-gradient` over the whole screen, subtle opacity ~0.03
2. **ARCHIVEFLOW logo** — large, all-caps, monospace font, cyan colour, running glitch keyframe animation (random X-shift + `clip-path` horizontal slice + RGB split `text-shadow`)
3. **Step list** — left-aligned, monospace, prefix `>_`; 6 steps, each transitioning: pending (gray) → active (cyan, blinking cursor) → done (dimmed + `[OK]` in magenta)
4. **Progress bar** — 2px tall, full width, cyan fill advancing with each step, scan-light shimmer, percentage label right-aligned
5. **Footer** — `SYS_BOOT v0.1.0 // ARCHIVEFLOW`, very small, low-opacity gray

### Animations (pure CSS `@keyframes`, no JS animation libraries)
- `glitch-shift`: alternates `transform: translateX` + `clip-path` cuts at irregular intervals, with magenta/cyan `text-shadow` offsets — runs continuously on the logo
- `blink-cursor`: opacity 0↔1 at 0.7s interval — on the active step cursor `_`
- `scanline-drift`: slow vertical `background-position` shift on the CRT overlay
- `progress-shine`: `background-position` sweep on the progress bar fill

## Data Flow

```
App.tsx mounts
  └─ isBooting = true  →  render <BootScreen onComplete={() => setIsBooting(false)} />
       │
       ├─ Step 1: await i18n.isInitialized (or short delay)          "初始化运行时环境"
       ├─ Step 2: await api.getStats()                                "建立数据库连接"
       ├─ Step 3: await taskStore.fetchTasks()                        "加载任务列表"
       ├─ Step 4: await api.getAuditEvents({ limit: 50 })             "读取审计记录"
       ├─ Step 5: read localStorage prefs (sync, instant)             "同步应用配置"
       └─ Step 6: setTimeout 400ms                                    "系统就绪"
            │
            └─ call onComplete()  →  fade-out CSS transition  →  isBooting = false
```

Each step sets `currentStep` index in local state; progress bar = `(currentStep / 6) * 100`.

## Files Changed

| File | Change |
|------|--------|
| `src/components/BootScreen.tsx` | New component — full boot overlay |
| `src/App.tsx` | Add `isBooting` state, conditionally render `<BootScreen>` |
| `src/index.css` | Add `@keyframes` for glitch, blink, scanline, progress-shine |
| `src/i18n/index.ts` | Add boot step strings (zh + en) |

## Constraints

- Zero new npm dependencies — pure CSS animations + existing lucide-react icons if needed
- Must respect `prefers-reduced-motion` (disable glitch + blink animations, keep steps and progress bar)
- Works in both light and dark OS theme (boot screen is always dark — it's its own world)
- `BootScreen` receives `onComplete: () => void` prop; all async logic lives inside the component
- After `onComplete`, App renders normally; BootScreen is unmounted (not hidden)
