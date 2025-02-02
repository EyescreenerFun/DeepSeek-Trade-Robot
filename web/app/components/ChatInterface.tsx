"use client"
import { useChat } from "ai/react"
import { deepseek } from "@ai-sdk/deepseek"

export default function ChatInterface() {
  const { messages, input, handleInputChange, handleSubmit } = useChat({
    api: "/api/chat",
    model: deepseek("deepseek-chat"),
  })

  return (
    <div className="border rounded-lg p-4 h-[500px] flex flex-col">
      <h2 className="text-2xl font-semibold mb-4">AI Assistant</h2>
      <div className="flex-grow overflow-auto mb-4">
        {messages.map((message, i) => (
          <div key={i} className={`mb-2 ${message.role === "user" ? "text-blue-600" : "text-green-600"}`}>
            <strong>{message.role === "user" ? "You: " : "AI: "}</strong>
            {message.content}
          </div>
        ))}
      </div>
      <form onSubmit={handleSubmit} className="flex">
        <input
          className="flex-grow border rounded-l-lg px-2 py-1"
          value={input}
          onChange={handleInputChange}
          placeholder="Ask about crypto trading..."
        />
        <button className="bg-blue-500 text-white px-4 py-1 rounded-r-lg" type="submit">
          Send
        </button>
      </form>
    </div>
  )
}

