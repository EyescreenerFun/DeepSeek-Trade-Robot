import { deepseek } from "@ai-sdk/deepseek"
import { streamText } from "ai"

export const runtime = "edge"

export async function POST(req: Request) {
  const { messages } = await req.json()
  const response = await streamText({
    model: deepseek("deepseek-chat"),
    messages,
    temperature: 0.7,
    max_tokens: 800,
  })

  return new Response(response.stream)
}

