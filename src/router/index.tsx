/**
 * @fileoverview 文件功能：本文件提供 index.tsx 模块的实现
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies react-router-dom
 */

import { BrowserRouter, Routes, Route } from "react-router-dom"
import MainLayout from "@/layouts/MainLayout"
import HomePage from "@/pages/HomePage"
import TaskPage from "@/pages/TaskPage"
import TaskDetailPage from "@/pages/TaskDetailPage"
import ReportPage from "@/pages/ReportPage"
import SettingsPage from "@/pages/SettingsPage"

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export default function AppRouter() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<MainLayout />}>
          <Route path="/" element={<HomePage />} />
          <Route path="/tasks" element={<TaskPage />} />
          <Route path="/tasks/:taskId" element={<TaskDetailPage />} />
          <Route path="/reports" element={<ReportPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  )
}
