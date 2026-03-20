import { BrowserRouter, Routes, Route } from "react-router-dom"
import MainLayout from "@/layouts/MainLayout"
import HomePage from "@/pages/HomePage"
import TaskPage from "@/pages/TaskPage"
import TaskDetailPage from "@/pages/TaskDetailPage"
import ReportPage from "@/pages/ReportPage"
import SettingsPage from "@/pages/SettingsPage"

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
