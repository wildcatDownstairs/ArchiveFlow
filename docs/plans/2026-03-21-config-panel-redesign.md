# Config Panel Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Redesign the `canStart` configuration panel in `RecoveryPanel.tsx` (L953–L1261) to match the reference HTML's minimal dark floating-card aesthetic, translated to Tailwind classes.

**Architecture:** All changes are purely presentational — same state, same props, same logic. We replace the wrapper `div`, tab nav, and all inner panel content with Tailwind equivalents of the reference HTML's CSS design language. No new components, no new state, no new props.

**Tech Stack:** React 19, Tailwind CSS v4, shadcn/ui, Lucide icons (already imported)

---

### Design Language Reference (HTML → Tailwind)

| HTML CSS concept | Tailwind equivalent |
|---|---|
| `--surface: #16161c` outer wrapper | `bg-muted/30 rounded-2xl p-1` (no border) |
| `.tab-nav` pill container | `flex gap-1 bg-muted/40 rounded-xl p-1.5 mb-7` |
| `.tab-btn` | `flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-[10px] text-sm font-medium text-muted-foreground transition-colors hover:text-foreground hover:bg-muted/50` |
| `.tab-btn.active` | `bg-primary/10 text-primary` |
| `.section` block | `bg-muted/30 rounded-2xl p-6 mb-3` |
| `.section-label` | `text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4` |
| `.dict-textarea` | `w-full bg-muted/50 rounded-xl px-4 py-4 text-sm font-mono resize-y min-h-[140px] border-none outline-none focus:bg-muted/70 placeholder:text-muted-foreground/50` |
| `.import-row` | `flex items-center justify-between mb-3` |
| `.btn-ghost` | `inline-flex items-center gap-1.5 bg-muted/50 rounded-lg px-3.5 py-1.5 text-xs text-muted-foreground hover:bg-muted/70 hover:text-foreground transition-colors` |
| `.count-pill` | `inline-flex items-center gap-1.5 bg-muted/50 rounded-full px-3 py-1 text-xs text-muted-foreground` |
| `.check-item` row | `flex items-center gap-2.5 px-3.5 py-2.5 rounded-[10px] cursor-pointer hover:bg-muted/50 transition-colors select-none` |
| custom checkbox box | `w-[18px] h-[18px] rounded-[5px] bg-muted/70 flex-shrink-0 flex items-center justify-center` checked: `bg-primary` |
| `.charset-tag` | `flex items-center gap-2 px-4 py-2.5 rounded-[10px] bg-muted/50 cursor-pointer transition-colors hover:bg-muted/70` selected: `bg-primary/10` |
| `.charset-indicator` dot | `w-2 h-2 rounded-full bg-muted-foreground/40 flex-shrink-0` selected: `bg-primary` |
| `.charset-tag-label` | `text-sm text-muted-foreground` selected: `text-primary` |
| `.charset-preview` | `text-xs text-muted-foreground/60 font-mono mt-5` |
| `.range-row` | `grid grid-cols-2 gap-3` |
| `.field-group label` | `block text-[11px] font-semibold uppercase tracking-wider text-muted-foreground mb-2` |
| `.field-input` | `w-full bg-muted/50 rounded-[10px] px-3.5 py-3 text-[15px] font-medium border-none outline-none focus:bg-muted/70 transition-colors` |
| `.mask-input` | `w-full bg-muted/50 rounded-xl px-4 py-3.5 text-primary font-mono text-lg font-medium tracking-wider border-none outline-none focus:bg-muted/70 transition-colors` |
| `.mask-hints` | `flex flex-wrap gap-1.5 mt-2.5` |
| `.mask-hint-chip` | `bg-muted/50 rounded-md px-2.5 py-1 text-[11.5px] font-mono text-muted-foreground` `<strong>` → `font-medium text-primary` |
| `.priority-row` | `flex items-center gap-3` |
| `.priority-stepper` | `flex items-center bg-muted/50 rounded-[10px] overflow-hidden` |
| `.step-btn` | `w-9 h-9 flex items-center justify-center text-lg text-muted-foreground hover:bg-muted/70 hover:text-foreground transition-colors` |
| `.step-val` | `min-w-[40px] text-center text-[15px] font-medium` |
| `.backend-card` | `p-4 rounded-xl bg-muted/50 cursor-pointer transition-colors hover:bg-muted/70 select-none` selected: `bg-primary/10` |
| `.backend-radio` dot | `w-4 h-4 rounded-full border-2 border-muted-foreground/40 flex items-center justify-center flex-shrink-0` selected: `border-primary` inner dot `w-2 h-2 rounded-full bg-primary` |
| `.backend-name` | `text-sm font-medium text-muted-foreground` selected: `text-primary` |
| `.backend-desc` | `text-[11.5px] text-muted-foreground/60 leading-relaxed` |
| `.start-btn` | `w-full rounded-2xl py-4 bg-primary text-white font-semibold text-[15px] flex items-center justify-center gap-2 shadow-lg shadow-primary/25 hover:shadow-primary/40 hover:-translate-y-px transition-all mt-4` |

---

### Task 1: Outer wrapper + Tab Nav

**Files:**
- Modify: `src/components/RecoveryPanel.tsx:953-993`

**Step 1: Replace outer container + tab nav**

Replace L954–L993 (the outer `<div className="rounded-lg border p-4 space-y-4">` and the `<div className="flex border-b">` tab nav) with:

```tsx
<div className="space-y-3">
  {/* 模式选择 Tab - pill 风格 */}
  <div className="flex gap-1 bg-muted/40 rounded-xl p-1.5 mb-7">
    <button
      onClick={() => setActiveTab("dictionary")}
      className={cn(
        "flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-[10px] text-sm font-medium transition-colors",
        activeTab === "dictionary"
          ? "bg-primary/10 text-primary"
          : "text-muted-foreground hover:text-foreground hover:bg-muted/50",
      )}
    >
      <BookOpen className="h-4 w-4 flex-shrink-0" />
      {t("dictionary_attack")}
    </button>
    <button
      onClick={() => setActiveTab("bruteforce")}
      className={cn(
        "flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-[10px] text-sm font-medium transition-colors",
        activeTab === "bruteforce"
          ? "bg-primary/10 text-primary"
          : "text-muted-foreground hover:text-foreground hover:bg-muted/50",
      )}
    >
      <Zap className="h-4 w-4 flex-shrink-0" />
      {t("bruteforce_attack")}
    </button>
    <button
      onClick={() => setActiveTab("mask")}
      className={cn(
        "flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-[10px] text-sm font-medium transition-colors",
        activeTab === "mask"
          ? "bg-primary/10 text-primary"
          : "text-muted-foreground hover:text-foreground hover:bg-muted/50",
      )}
    >
      <KeyRound className="h-4 w-4 flex-shrink-0" />
      {t("mask_attack")}
    </button>
  </div>
```

**Step 2: Run tests**

```
npx vitest run
```

Expected: All tests pass (no structural change yet to inner content).

---

### Task 2: Dictionary panel redesign

**Files:**
- Modify: `src/components/RecoveryPanel.tsx:995-1073`

**Step 1: Replace dictionary panel content (L996–L1073)**

Replace the `{activeTab === "dictionary" && (...)}` block with:

```tsx
{/* 字典模式 */}
{activeTab === "dictionary" && (
  <div className="space-y-3">
    {/* 字典输入区 */}
    <div className="bg-muted/30 rounded-2xl p-6">
      <div className="flex items-center justify-between mb-3">
        <span className="inline-flex items-center gap-1.5 bg-muted/50 rounded-full px-3 py-1 text-xs text-muted-foreground">
          <FileUp className="h-[11px] w-[11px]" />
          {wordlistText.split("\n").map((l) => l.trim()).filter(Boolean).length}{" "}
          {t("items")}
        </span>
        <button
          onClick={() => void handleImportDictionaryFile()}
          className="inline-flex items-center gap-1.5 bg-muted/50 rounded-lg px-3.5 py-1.5 text-xs text-muted-foreground hover:bg-muted/70 hover:text-foreground transition-colors"
        >
          <FileUp className="h-3 w-3" />
          {t("import_dictionary_file")}
        </button>
      </div>
      <textarea
        value={wordlistText}
        onChange={(e) => {
          const nextText = e.target.value
          setWordlistText(nextText)
          updateRecoveryDrafts({ dictionaryText: nextText })
        }}
        placeholder={t("dictionary_placeholder")}
        rows={6}
        className="w-full bg-muted/50 rounded-xl px-4 py-4 text-sm font-mono resize-y min-h-[140px] outline-none focus:bg-muted/70 placeholder:text-muted-foreground/50 transition-colors"
      />
      {recoveryDrafts.dictionarySourceName && (
        <p className="text-xs text-muted-foreground mt-2">
          {recoveryDrafts.dictionarySourceName}
        </p>
      )}
    </div>

    {/* 变体生成 */}
    <div className="bg-muted/30 rounded-2xl p-6">
      <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4">
        {t("dictionary_generation_hint")}
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
        {(
          [
            ["capitalize", t("transform_capitalize")],
            ["uppercase", t("transform_uppercase")],
            ["leetspeak", t("transform_leetspeak")],
            ["reverse", t("transform_reverse")],
            ["duplicate", t("transform_duplicate")],
            ["yearPatterns", t("transform_year_patterns")],
            ["separatorPatterns", t("transform_separator_patterns"), !dictionaryOptions.combineWords] as const,
            ["commonSuffixes", t("transform_common_suffixes")],
            ["combineWords", t("combine_dictionary")],
            ["includeFilenamePatterns", t("include_filename_patterns")],
          ] as const
        ).map(([key, label, disabled]) => (
          <label
            key={key}
            className={cn(
              "flex items-center gap-2.5 px-3.5 py-2.5 rounded-[10px] select-none transition-colors",
              disabled
                ? "cursor-not-allowed opacity-40"
                : "cursor-pointer hover:bg-muted/50",
            )}
          >
            <input
              type="checkbox"
              checked={dictionaryOptions[key]}
              disabled={!!disabled}
              onChange={(e) =>
                setDictionaryOptions((prev) => ({ ...prev, [key]: e.target.checked }))
              }
              className="sr-only"
            />
            {/* custom checkbox box */}
            <span
              className={cn(
                "w-[18px] h-[18px] rounded-[5px] flex-shrink-0 flex items-center justify-center transition-colors",
                dictionaryOptions[key] && !disabled
                  ? "bg-primary"
                  : "bg-muted/70",
              )}
            >
              {dictionaryOptions[key] && !disabled && (
                <svg width="9" height="6" viewBox="0 0 9 6" fill="none">
                  <path d="M1 3L3.5 5.5L8 1" stroke="white" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              )}
            </span>
            <span className={cn(
              "text-sm",
              dictionaryOptions[key] ? "text-foreground" : "text-muted-foreground",
            )}>
              {label}
            </span>
          </label>
        ))}
      </div>
    </div>
  </div>
)}
```

**Step 2: Run tests**

```
npx vitest run
```

Expected: All pass.

---

### Task 3: Bruteforce panel redesign

**Files:**
- Modify: `src/components/RecoveryPanel.tsx:1075-1180`

**Step 1: Replace bruteforce panel content**

Replace the `{activeTab === "bruteforce" && (...)}` block with:

```tsx
{/* 暴力破解模式 */}
{activeTab === "bruteforce" && (
  <div className="space-y-3">
    {/* 字符集 */}
    <div className="bg-muted/30 rounded-2xl p-6">
      <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4">
        {t("charset")}
      </div>
      <div className="flex flex-wrap gap-2 mb-5">
        {(["lowercase", "uppercase", "digits", "special"] as const).map((key) => {
          const selected = !useCustomCharset && charsetFlags[key]
          return (
            <label
              key={key}
              className={cn(
                "flex items-center gap-2 px-4 py-2.5 rounded-[10px] cursor-pointer select-none transition-colors",
                selected ? "bg-primary/10" : "bg-muted/50 hover:bg-muted/70",
              )}
              onClick={() => {
                setUseCustomCharset(false)
                setCharsetFlags((prev) => ({ ...prev, [key]: !prev[key] }))
              }}
            >
              <span className={cn(
                "w-2 h-2 rounded-full flex-shrink-0 transition-colors",
                selected ? "bg-primary" : "bg-muted-foreground/40",
              )} />
              <span className={cn("text-sm", selected ? "text-primary" : "text-muted-foreground")}>
                {t(`charset_${key}`)}
              </span>
            </label>
          )
        })}
        <label
          className={cn(
            "flex items-center gap-2 px-4 py-2.5 rounded-[10px] cursor-pointer select-none transition-colors",
            useCustomCharset ? "bg-primary/10" : "bg-muted/50 hover:bg-muted/70",
          )}
          onClick={() => setUseCustomCharset((v) => !v)}
        >
          <span className={cn(
            "w-2 h-2 rounded-full flex-shrink-0 transition-colors",
            useCustomCharset ? "bg-primary" : "bg-muted-foreground/40",
          )} />
          <span className={cn("text-sm", useCustomCharset ? "text-primary" : "text-muted-foreground")}>
            {t("charset_custom")}
          </span>
        </label>
      </div>
      {useCustomCharset && (
        <input
          type="text"
          value={customCharset}
          onChange={(e) => setCustomCharset(e.target.value)}
          placeholder={t("charset_custom_placeholder")}
          className="w-full bg-muted/50 rounded-xl px-4 py-3 text-sm font-mono outline-none focus:bg-muted/70 transition-colors mb-3"
        />
      )}
      <p className="text-xs text-muted-foreground/60 font-mono leading-relaxed">
        {buildCharset().slice(0, 60)}
        {buildCharset().length > 60 ? "…" : ""}
        {"  ·  "}
        {buildCharset().length} chars
      </p>
    </div>

    {/* 密码长度 */}
    <div className="bg-muted/30 rounded-2xl p-6">
      <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4">
        {t("min_length")} / {t("max_length")}
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="block text-[11px] font-semibold uppercase tracking-wider text-muted-foreground mb-2">
            {t("min_length")}
          </label>
          <input
            type="number"
            value={minLength}
            onChange={(e) => setMinLength(Math.max(1, parseInt(e.target.value) || 1))}
            min={1}
            max={12}
            className="w-full bg-muted/50 rounded-[10px] px-3.5 py-3 text-[15px] font-medium outline-none focus:bg-muted/70 transition-colors"
          />
        </div>
        <div>
          <label className="block text-[11px] font-semibold uppercase tracking-wider text-muted-foreground mb-2">
            {t("max_length")}
          </label>
          <input
            type="number"
            value={maxLength}
            onChange={(e) => setMaxLength(Math.max(1, parseInt(e.target.value) || 1))}
            min={1}
            max={12}
            className="w-full bg-muted/50 rounded-[10px] px-3.5 py-3 text-[15px] font-medium outline-none focus:bg-muted/70 transition-colors"
          />
        </div>
      </div>
    </div>
  </div>
)}
```

**Step 2: Run tests**

```
npx vitest run
```

---

### Task 4: Mask panel redesign

**Files:**
- Modify: `src/components/RecoveryPanel.tsx:1182-1198`

**Step 1: Replace mask panel content**

```tsx
{/* 掩码模式 */}
{activeTab === "mask" && (
  <div className="space-y-3">
    <div className="bg-muted/30 rounded-2xl p-6">
      <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4">
        {t("mask_pattern")}
      </div>
      <input
        type="text"
        value={maskPattern}
        onChange={(e) => setMaskPattern(e.target.value)}
        placeholder={t("mask_placeholder")}
        spellCheck={false}
        className="w-full bg-muted/50 rounded-xl px-4 py-3.5 text-primary font-mono text-lg font-medium tracking-wider outline-none focus:bg-muted/70 transition-colors"
      />
      <div className="flex flex-wrap gap-1.5 mt-2.5">
        {[
          ["?l", t("charset_lowercase")],
          ["?u", t("charset_uppercase")],
          ["?d", t("charset_digits")],
          ["?s", t("charset_special")],
          ["?a", t("mask_any")],
          ["??", t("mask_literal_q")],
        ].map(([key, desc]) => (
          <span
            key={key}
            className="bg-muted/50 rounded-md px-2.5 py-1 text-[11.5px] font-mono text-muted-foreground"
          >
            <strong className="font-medium text-primary">{key}</strong>{" "}{desc}
          </span>
        ))}
      </div>
    </div>
  </div>
)}
```

Note: Requires adding `mask_any` and `mask_literal_q` i18n keys.

**Step 2: Run tests**

```
npx vitest run
```

---

### Task 5: Priority stepper + Backend cards

**Files:**
- Modify: `src/components/RecoveryPanel.tsx:1200-1251`

**Step 1: Replace shared priority + backend blocks**

Replace L1200–L1251 with the following two shared blocks (shown outside the tab panels):

```tsx
{/* 优先级 stepper */}
<div className="bg-muted/30 rounded-2xl p-6">
  <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4">
    {t("scheduler_priority")}
  </div>
  <div className="flex items-center gap-3">
    <span className="text-sm text-muted-foreground">{t("scheduler_priority")}</span>
    <div className="flex items-center bg-muted/50 rounded-[10px] overflow-hidden">
      <button
        type="button"
        onClick={() => setPriority((p) => Math.max(0, p - 1))}
        className="w-9 h-9 flex items-center justify-center text-lg text-muted-foreground hover:bg-muted/70 hover:text-foreground transition-colors"
        aria-label="decrease priority"
      >
        −
      </button>
      <span className="min-w-[40px] text-center text-[15px] font-medium">
        {priority}
      </span>
      <button
        type="button"
        onClick={() => setPriority((p) => p + 1)}
        className="w-9 h-9 flex items-center justify-center text-lg text-muted-foreground hover:bg-muted/70 hover:text-foreground transition-colors"
        aria-label="increase priority"
      >
        +
      </button>
    </div>
  </div>
</div>

{/* 恢复后端 */}
<div className="bg-muted/30 rounded-2xl p-6">
  <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground mb-4">
    {t("recovery_backend")}
  </div>
  <div className="grid grid-cols-2 gap-2.5">
    {/* CPU */}
    <div
      role="radio"
      aria-checked={backend === "cpu"}
      aria-label={t("recovery_backend_cpu")}
      tabIndex={0}
      onClick={() => setBackend("cpu")}
      onKeyDown={(e) => e.key === "Enter" && setBackend("cpu")}
      className={cn(
        "p-4 rounded-xl cursor-pointer select-none transition-colors",
        backend === "cpu" ? "bg-primary/10" : "bg-muted/50 hover:bg-muted/70",
      )}
    >
      <div className="flex items-center gap-2.5 mb-1">
        <span className={cn(
          "w-4 h-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center transition-colors",
          backend === "cpu" ? "border-primary" : "border-muted-foreground/40",
        )}>
          {backend === "cpu" && (
            <span className="w-2 h-2 rounded-full bg-primary block" />
          )}
        </span>
        <span className={cn("text-sm font-medium", backend === "cpu" ? "text-primary" : "text-muted-foreground")}>
          {t("recovery_backend_cpu")}
        </span>
      </div>
      <p className="text-[11.5px] text-muted-foreground/60 leading-relaxed pl-[26px]">
        {t("recovery_backend_cpu_hint")}
      </p>
    </div>
    {/* GPU */}
    <div
      role="radio"
      aria-checked={backend === "gpu"}
      aria-label={t("recovery_backend_gpu")}
      tabIndex={supportsGpuBackend ? 0 : -1}
      onClick={() => supportsGpuBackend && setBackend("gpu")}
      onKeyDown={(e) => e.key === "Enter" && supportsGpuBackend && setBackend("gpu")}
      className={cn(
        "p-4 rounded-xl select-none transition-colors",
        !supportsGpuBackend
          ? "opacity-50 cursor-not-allowed"
          : backend === "gpu"
            ? "bg-primary/10 cursor-pointer"
            : "bg-muted/50 hover:bg-muted/70 cursor-pointer",
      )}
    >
      <div className="flex items-center gap-2.5 mb-1">
        <span className={cn(
          "w-4 h-4 rounded-full border-2 flex-shrink-0 flex items-center justify-center transition-colors",
          backend === "gpu" ? "border-primary" : "border-muted-foreground/40",
        )}>
          {backend === "gpu" && (
            <span className="w-2 h-2 rounded-full bg-primary block" />
          )}
        </span>
        <span className={cn("text-sm font-medium", backend === "gpu" ? "text-primary" : "text-muted-foreground")}>
          {t("recovery_backend_gpu")}
        </span>
      </div>
      <p className="text-[11.5px] text-muted-foreground/60 leading-relaxed pl-[26px]">
        {t("recovery_backend_gpu_hint")}
      </p>
    </div>
  </div>
</div>
```

Note: Requires adding `recovery_backend_cpu_hint` i18n key (currently only gpu has a hint).

**Step 2: Run tests**

```
npx vitest run
```

---

### Task 6: Start button redesign

**Files:**
- Modify: `src/components/RecoveryPanel.tsx:603-621`

**Step 1: Replace start button classes**

```tsx
<button
  onClick={() => void handleStart()}
  disabled={isStarting}
  className={cn(
    "w-full rounded-2xl py-4 font-semibold text-[15px] flex items-center justify-center gap-2 shadow-lg transition-all",
    isStarting
      ? "bg-primary/60 cursor-not-allowed text-white/80 shadow-primary/15"
      : "bg-primary text-white shadow-primary/25 hover:shadow-primary/40 hover:-translate-y-px",
  )}
>
```

**Step 2: Run tests**

```
npx vitest run
```

---

### Task 7: Add missing i18n keys

**Files:**
- Modify: `src/i18n/index.ts`

Need to add:
- `mask_any` → 任意字符 / any char
- `mask_literal_q` → 问号本身 / literal ?
- `recovery_backend_cpu_hint` → 跨平台，支持暂停与继续 / Cross-platform, supports pause and resume

**Step 1: Add keys**

Find the `mask_hint` entry and add after it. Find `recovery_backend_gpu_hint` and add `recovery_backend_cpu_hint` nearby.

**Step 2: Run tests**

```
npx vitest run
```

Expected: All 72+ tests pass.

---

### Task 8: Commit

```bash
git add src/components/RecoveryPanel.tsx src/i18n/index.ts
git commit -m "重设计配置面板：无边框漂浮卡片风格（参考 UI 设计稿）"
```

---

## Execution Handoff

**Plan complete and saved to `docs/plans/2026-03-21-config-panel-redesign.md`.**

**Two execution options:**

1. **Subagent-Driven (this session)** — dispatch fresh subagent per task, review between tasks, fast iteration
2. **Parallel Session (separate)** — open new session with executing-plans, batch execution with checkpoints

**Which approach?**
