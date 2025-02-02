"use client"

import { useState } from "react"

export default function TradingInterface() {
  const [amount, setAmount] = useState("")
  const [currency, setCurrency] = useState("BTC")

  const handleTrade = (action: "buy" | "sell") => {
    // 这里应该实现实际的交易逻辑
    console.log(`${action} ${amount} ${currency}`)
    alert(`${action.toUpperCase()} order placed for ${amount} ${currency}`)
  }

  return (
    <div className="border rounded-lg p-4 h-[500px]">
      <h2 className="text-2xl font-semibold mb-4">Trading Interface</h2>
      <div className="mb-4">
        <label className="block mb-2">Amount:</label>
        <input
          type="number"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          className="w-full border rounded px-2 py-1"
        />
      </div>
      <div className="mb-4">
        <label className="block mb-2">Currency:</label>
        <select
          value={currency}
          onChange={(e) => setCurrency(e.target.value)}
          className="w-full border rounded px-2 py-1"
        >
          <option value="BTC">Bitcoin (BTC)</option>
          <option value="ETH">Ethereum (ETH)</option>
          <option value="USDT">Tether (USDT)</option>
        </select>
      </div>
      <div className="flex justify-between">
        <button onClick={() => handleTrade("buy")} className="bg-green-500 text-white px-4 py-2 rounded">
          Buy
        </button>
        <button onClick={() => handleTrade("sell")} className="bg-red-500 text-white px-4 py-2 rounded">
          Sell
        </button>
      </div>
    </div>
  )
}

