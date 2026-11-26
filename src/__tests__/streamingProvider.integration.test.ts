import { fetchModelStream } from '../providers/streaming';

describe('streaming provider HTTP integration', () => {
  const originalFetch = globalThis.fetch;

  afterEach(() => {
    globalThis.fetch = originalFetch;
    jest.restoreAllMocks();
  });

  it('posts a streaming request and forwards text deltas without waiting for a full JSON response', async () => {
    const encoder = new TextEncoder();
    const responseBody = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode('data: {"choices":[{"delta":{"content":"first"}}]}\n\n'));
        controller.enqueue(encoder.encode('data: {"choices":[{"delta":{"content":" second"}}]}\n\n'));
        controller.enqueue(encoder.encode('data: [DONE]\n\n'));
        controller.close();
      }
    });
    const fetchMock = jest.fn().mockResolvedValue(new Response(responseBody, {
      status: 200,
      headers: { 'content-type': 'text/event-stream' }
    }));
    globalThis.fetch = fetchMock as typeof fetch;
    const chunks: string[] = [];

    await fetchModelStream('https://provider.example/v1/chat/completions', {
      method: 'POST',
      headers: { Authorization: 'Bearer test-key' },
      body: JSON.stringify({ model: 'test-model', stream: true })
    }, 'openai-sse', chunk => chunks.push(chunk));

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe('https://provider.example/v1/chat/completions');
    expect(JSON.parse(String(fetchMock.mock.calls[0][1]?.body))).toMatchObject({ model: 'test-model', stream: true });
    expect(chunks).toEqual(['first', ' second']);
  });

  it('surfaces provider HTTP failures instead of returning an empty success', async () => {
    globalThis.fetch = jest.fn().mockResolvedValue(new Response('invalid API key', { status: 401 })) as typeof fetch;
    await expect(fetchModelStream('https://provider.example/chat/completions', { method: 'POST' }, 'openai-sse', () => undefined))
      .rejects.toThrow('Streaming request failed (401): invalid API key');
  });

  it('supports Ollama NDJSON over the same streaming transport', async () => {
    const response = new Response('{"message":{"content":"local "}}\n{"message":{"content":"model"}}\n', { status: 200 });
    globalThis.fetch = jest.fn().mockResolvedValue(response) as typeof fetch;
    const chunks: string[] = [];
    await fetchModelStream('http://127.0.0.1:11434/api/chat', {
      method: 'POST',
      body: JSON.stringify({ model: 'llama3.1', stream: true })
    }, 'ollama-ndjson', chunk => chunks.push(chunk));
    expect(chunks).toEqual(['local ', 'model']);
  });
});
