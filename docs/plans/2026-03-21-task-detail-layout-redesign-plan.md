# TaskDetailPage 布局重设计 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 重构 `TaskDetailPage` 为"左侧主区 RecoveryPanel + 右侧固定侧边栏（元信息 + 文件树）"布局，消除左侧空旷问题，提升紧凑感。

**Architecture:** 仅修改 `src/pages/TaskDetailPage.tsx`，将原来的独立信息条和时间戳行合并为右侧 `<aside>` 侧边栏；主区域改为 flex 横排，`xl` 断点以下降级为单列。RecoveryPanel 本身不做任何改动。

**Tech Stack:** React 19, Tailwind CSS v4, lucide-react, Tauri 2

---

### Task 1: 重构主体布局骨架

将原来的 `grid xl:grid-cols-12` 双列替换为 `flex xl:flex-row flex-col`，左侧主区 `flex-1 min-w-0`，右侧侧边栏 `w-72 flex-shrink-0`。

**Files:**
- Modify: `src/pages/TaskDetailPage.tsx`

**Step 1: 确认测试基线可通过**

```bash
npx vitest run --reporter=verbose src/pages/__tests__/TaskDetailPage.test.tsx 2>/dev/null || npx vitest run
```
Expected: 所有测试 PASS（当前布局未改动前确认基线）

**Step 2: 替换主体 div 骨架**

找到（约 411 行）：
```tsx
      {/* 主体内容：双列布局 */}
      <div className="grid grid-cols-1 xl:grid-cols-12 gap-6 items-start">
```
替换为：
```tsx
      {/* 主体内容：主区 + 右侧边栏 */}
      <div className="flex flex-col xl:flex-row gap-6 items-start">
```

**Step 3: 替换左侧列 class**

找到（约 415 行）：
```tsx
          <div
            className={cn(
              "flex flex-col gap-6",
              (task.archive_type === "zip" || task.archive_type === "sevenz" || task.archive_type === "rar") && info?.is_encrypted
                ? "xl:col-span-5"
                : "xl:col-span-12"
            )}
          >
```
替换为：
```tsx
          <div className="flex flex-col gap-6 flex-1 min-w-0">
```

**Step 4: 替换右侧 RecoveryPanel 列 class**

找到（约 448 行）：
```tsx
          <div className="xl:col-span-7 flex flex-col gap-6">
```
替换为：
```tsx
          <div className="flex flex-col gap-6 flex-1 min-w-0">
```

**Step 5: 运行测试验证无破坏**

```bash
npx vitest run
```
Expected: PASS

**Step 6: Commit**

```bash
git add src/pages/TaskDetailPage.tsx
git commit -m "refactor(ui): 主体布局从 grid 改为 flex 横排骨架"
```

---

### Task 2: 新增右侧侧边栏，迁移元信息

将原来的独立"综合信息条"card、时间戳行、错误信息 banner 迁移到右侧 `<aside>` 侧边栏。

**Files:**
- Modify: `src/pages/TaskDetailPage.tsx`

**Step 1: 删除原综合信息条 card（约 293-395 行）**

删除以下整块（含注释）：
```tsx
      {/* 综合信息条 */}
      <div className="rounded-lg border bg-card shadow-sm p-4 md:p-5">
        ...
      </div>
```

**Step 2: 删除原时间戳行（约 397-401 行）**

删除：
```tsx
      {/* 时间信息 */}
      <div className="flex gap-6 text-sm text-muted-foreground">
        <span>{t("created_at")}: {formatDateTime(task.created_at)}</span>
        <span>{t("updated_at")}: {formatDateTime(task.updated_at)}</span>
      </div>
```

**Step 3: 删除原错误信息 banner（约 403-408 行）**

删除：
```tsx
      {/* 错误信息 */}
      {task.error_message && (
        <div className="rounded-md bg-red-50 border border-red-200 p-3 text-red-700 text-sm">
          {task.error_message}
        </div>
      )}
```

**Step 4: 在主体 flex 容器里，RecoveryPanel 侧之后（或之前，取决于是否有 RecoveryPanel）增加右侧 `<aside>`**

在主体 `<div className="flex flex-col xl:flex-row gap-6 items-start">` 的**最后**（即闭合 `</div>` 前），插入：

```tsx
        {/* 右侧侧边栏：元信息 + 文件树 */}
        <aside className="w-full xl:w-72 flex-shrink-0 flex flex-col gap-4">
          {/* 元信息卡片 */}
          <div className="rounded-lg border bg-card shadow-sm p-4 space-y-3">
            <div className="grid grid-cols-2 gap-x-3 gap-y-2.5 text-sm">
              {/* 类型 */}
              <span className="text-muted-foreground flex items-center gap-1.5">
                <FileArchive className="h-3.5 w-3.5 flex-shrink-0" />
                {t("file_type")}
              </span>
              <span className={cn("inline-flex items-center rounded-full px-2 py-0.5 text-xs font-semibold w-fit", TYPE_BADGE_COLORS[task.archive_type])}>
                {t(`type_${task.archive_type}`)}
              </span>

              {/* 状态 */}
              <span className="text-muted-foreground flex items-center gap-1.5">
                <Zap className="h-3.5 w-3.5 flex-shrink-0" />
                {t("file_status")}
              </span>
              <span className={cn("inline-flex items-center rounded-full px-2 py-0.5 text-xs font-semibold w-fit", STATUS_BADGE_COLORS[task.status])}>
                {t(`status_${task.status}`)}
              </span>

              {/* 大小 */}
              <span className="text-muted-foreground flex items-center gap-1.5">
                <HardDrive className="h-3.5 w-3.5 flex-shrink-0" />
                {t("file_size")}
              </span>
              <span className="font-medium">{formatFileSize(task.file_size)}</span>

              {/* 加密状态 */}
              <span className="text-muted-foreground flex items-center gap-1.5">
                <ShieldCheck className="h-3.5 w-3.5 flex-shrink-0" />
                {t("encryption")}
              </span>
              <span>
                {info ? (
                  info.is_encrypted ? (
                    <span className="flex items-center gap-1 text-xs font-medium text-amber-600 dark:text-amber-500">
                      <ShieldAlert className="h-3.5 w-3.5" />
                      {t("encrypted")}
                    </span>
                  ) : (
                    <span className="flex items-center gap-1 text-xs font-medium text-green-600 dark:text-green-500">
                      <ShieldCheck className="h-3.5 w-3.5" />
                      {t("not_encrypted")}
                    </span>
                  )
                ) : (
                  <span className="text-muted-foreground">-</span>
                )}
              </span>

              {info && (
                <>
                  {/* 总文件数 */}
                  <span className="text-muted-foreground flex items-center gap-1.5">
                    <Files className="h-3.5 w-3.5 flex-shrink-0" />
                    {t("total_entries")}
                  </span>
                  <span className="font-medium">{info.total_entries}</span>

                  {/* 解压大小 */}
                  <span className="text-muted-foreground flex items-center gap-1.5">
                    <HardDrive className="h-3.5 w-3.5 flex-shrink-0" />
                    {t("uncompressed_size")}
                  </span>
                  <span className="font-medium">{formatFileSize(info.total_size)}</span>

                  {/* 加密文件数 */}
                  <span className="text-muted-foreground flex items-center gap-1.5">
                    <Lock className="h-3.5 w-3.5 flex-shrink-0" />
                    {t("encrypted_entries")}
                  </span>
                  <span className="font-medium">
                    <span className={info.entries.some((e) => e.is_encrypted) ? "text-amber-600 dark:text-amber-500" : ""}>
                      {info.entries.filter((e) => e.is_encrypted).length}
                    </span>
                    <span className="text-muted-foreground font-normal">
                      {" / "}{info.entries.filter((e) => !e.is_directory).length}
                    </span>
                  </span>
                </>
              )}
            </div>

            {/* 分割线 + 时间信息 */}
            <div className="border-t pt-3 space-y-1.5 text-xs text-muted-foreground">
              <div className="flex justify-between gap-2">
                <span>{t("created_at")}</span>
                <span className="text-right font-medium text-foreground">{formatDateTime(task.created_at)}</span>
              </div>
              <div className="flex justify-between gap-2">
                <span>{t("updated_at")}</span>
                <span className="text-right font-medium text-foreground">{formatDateTime(task.updated_at)}</span>
              </div>
            </div>
          </div>

          {/* 错误信息 */}
          {task.error_message && (
            <div className="rounded-md bg-red-50 border border-red-200 p-3 text-red-700 text-sm">
              {task.error_message}
            </div>
          )}

          {/* 归档内容（可折叠，仅在有条目时显示） */}
          {info && info.entries.length > 0 && (
            <section className="space-y-2">
              <button
                onClick={() => setIsFileTreeExpanded(!isFileTreeExpanded)}
                className="flex items-center gap-2 text-sm font-semibold hover:text-indigo-600 transition-colors focus:outline-none w-full text-left"
              >
                {isFileTreeExpanded ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
                {t("archive_contents")}
              </button>
              {isFileTreeExpanded && (
                <div className="max-h-64 rounded-lg border bg-card p-3 overflow-y-auto">
                  {fileTree.map((node) => (
                    <FileTreeNode key={node.path} node={node} t={t} />
                  ))}
                </div>
              )}
            </section>
          )}
        </aside>
```

**Step 5: 删除主体内容中原来的独立文件树 section（已经迁入 aside）**

在主体 flex 的左侧子 div（`<div className="flex flex-col gap-6 flex-1 min-w-0">`）里，如果仍有 `<section className="space-y-3">` 文件树，删除整个 `<section>` 块（约 422-443 行原代码）。

**Step 6: 调整标题区路径截断**

找到：
```tsx
          <p className="text-sm text-muted-foreground truncate max-w-xl">
```
替换为：
```tsx
          <p className="text-sm text-muted-foreground break-all">
```

**Step 7: 运行测试**

```bash
npx vitest run
```
Expected: PASS

**Step 8: Commit**

```bash
git add src/pages/TaskDetailPage.tsx
git commit -m "feat(ui): 详情页元信息/文件树迁入右侧侧边栏，路径截断修复"
```

---

### Task 3: 验证 RecoveryPanel 显示条件与 aside 的关系

当前逻辑：只有 `zip/sevenz/rar` 且 `is_encrypted` 才显示 RecoveryPanel。需要确保非加密文件时，`aside` 作为唯一内容也能正常显示。

**Files:**
- Modify: `src/pages/TaskDetailPage.tsx`

**Step 1: 检查非加密文件时主体 flex 容器内容**

非加密文件时：左侧 `<div className="flex-1 min-w-0">` 无内容（原文件树已迁入 aside），会是空 div。需要将这个空 div 的条件渲染去掉，让 `<aside>` 独立全宽显示（或用 `w-full`）。

在主体 flex 容器里，左侧 div 本身就有条件判断（`{info && info.entries.length > 0 && ...}`）——在无文件树时整个左侧 div 就不渲染了。

确认 RecoveryPanel 的显示条件判断独立存在，不依赖文件树。检查原代码确认二者是分开的 `{...}` 块。

**Step 2: 确保无 RecoveryPanel 时 aside 占满宽度**

当没有 RecoveryPanel（非加密文件）时，`<aside className="w-full xl:w-72 ...">` 在 xl 以上宽度会是 `w-72`，浪费了左侧空间。

修改 aside 的 class，让它在没有 RecoveryPanel 时也是 `w-full`：

提取变量：
```tsx
  const hasRecoveryPanel = (task.archive_type === "zip" || task.archive_type === "sevenz" || task.archive_type === "rar") && info?.is_encrypted
```

更新 aside class：
```tsx
          <aside className={cn(
            "flex-shrink-0 flex flex-col gap-4",
            hasRecoveryPanel ? "w-full xl:w-72" : "w-full xl:max-w-sm"
          )}>
```

**Step 3: 运行测试**

```bash
npx vitest run
```
Expected: PASS

**Step 4: Commit**

```bash
git add src/pages/TaskDetailPage.tsx
git commit -m "fix(ui): 无加密时 aside 占满全宽，修正空列问题"
```

---

### Task 4: 最终视觉检查与收尾

**Step 1: 运行完整测试套件**

```bash
npx vitest run
```
Expected: 全部 PASS

**Step 2: 检查 unused import**

`ChevronDown`、`ChevronRight` 已迁入 aside，确认仍然被引用。原来主区域的文件树 section 删除后，确认 `FileTreeNode` 组件仍在 aside 中被使用。

**Step 3: 运行 TypeScript 类型检查**

```bash
npx tsc --noEmit
```
Expected: 无错误

**Step 4: Commit 设计文档**

```bash
git add docs/plans/2026-03-21-task-detail-layout-redesign.md docs/plans/2026-03-21-task-detail-layout-redesign-plan.md
git commit -m "docs: 添加 TaskDetailPage 布局重设计文档与实现计划"
```
