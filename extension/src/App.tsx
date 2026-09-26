import { useState } from 'react'
import './App.css'

const BUTTONS = ['Button A', 'Button B', 'Button C']

function App() {
  const [lastClicked, setLastClicked] = useState<string | null>(null)

  return (
    <main className="test-page">
      <h1>Test Page</h1>
      <div className="button-row">
        {BUTTONS.map((label) => (
          <button
            key={label}
            type="button"
            className="test-button"
            onClick={() => setLastClicked(label)}
          >
            {label}
          </button>
        ))}
      </div>
      <p>Last clicked: {lastClicked ?? 'none'}</p>
    </main>
  )
}

export default App
