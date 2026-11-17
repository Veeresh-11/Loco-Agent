import { consumeModelStream } from '../providers/streaming';

function responseFrom(chunks: string[], status = 200): Response {
  const encoder = new TextEncoder();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    }
  });
  return new Response(body, { status });
}

describe('consumeModelStream', () => {
  it('emits OpenAI SSE deltas as they arrive and ignores [DONE]', async () => {
    const chunks: string[] = [];
    await consumeModelStream(responseFrom([
      'data: {"choices":[{"delta":{"content":"Hello"}}]}\n\n',
      'data: {"choices":[{"delta":{"content":" world"}}]}\n\n',
      'data: [DONE]\n\n'
    ]), 'openai-sse', chunk => chunks.push(chunk));
    expect(chunks).toEqual(['Hello', ' world']);
  });

  it('handles SSE records split across transport chunks', async () => {
    const chunks: string[] = [];
    await consumeModelStream(responseFrom([
      'data: {"choices":[{"delta":{"content":"spl',
      'it"}}]}\n\n'
    ]), 'openai-sse', chunk => chunks.push(chunk));
    expect(chunks).toEqual(['split']);
  });

  it('emits Ollama NDJSON message and response fields', async () => {
    const chunks: string[] = [];
    await consumeModelStream(responseFrom([
      '{"message":{"content":"A"}}\n{"response":"B","done":true}\n'
    ]), 'ollama-ndjson', chunk => chunks.push(chunk));
    expect(chunks).toEqual(['A', 'B']);
  });

  it('rejects unsuccessful responses with bounded response details', async () => {
    await expect(consumeModelStream(new Response('provider unavailable', { status: 503 }), 'openai-sse', () => undefined))
      .rejects.toThrow('Streaming request failed (503): provider unavailable');
  });

  it('rejects malformed JSON instead of silently truncating output', async () => {
    await expect(consumeModelStream(responseFrom(['data: {bad}\n\n']), 'openai-sse', () => undefined))
      .rejects.toThrow('Provider returned malformed streaming JSON.');
  });
});
