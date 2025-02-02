import ChatInterface from "./components/ChatInterface"
import TradingInterface from "./components/TradingInterface"

export default function Home() {
  return (
    <div className="w-full max-w-4xl mx-auto">
      <h1 className="text-4xl font-bold mb-8 text-center">Crypto AI Trader</h1>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
        <ChatInterface />
        <TradingInterface />
      </div>
    </div>
  )
}

