/**
 * @fileoverview 文件功能：本文件提供 App.tsx 模块的实现
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies react
 */

import { useState } from "react"
import AppRouter from "@/router"
import BootScreen from "@/components/BootScreen"

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
function App() {
  const [isBooting, setIsBooting] = useState(true)

  if (isBooting) {
    return <BootScreen onComplete={() => setIsBooting(false)} />
  }

  return <AppRouter />
}

export default App
