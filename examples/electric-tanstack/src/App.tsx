import { ElectricTanstackQueryProvider } from "@electric-sql/tanstack-react-query"

import { Example } from "./Example"
import logo from "./assets/logo.svg"
import "./App.css"
import "./style.css"

export default function App() {
  return (
    <div className="App">
      <header className="App-header">
        <img src={logo} className="App-logo" alt="logo" />

        <ElectricTanstackQueryProvider>
          <Example />
        </ElectricTanstackQueryProvider>
      </header>
    </div>
  )
}
