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
