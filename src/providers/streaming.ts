export type StreamFormat = 'openai-sse' | 'ollama-ndjson';

function extractText(payload: unknown, format: StreamFormat): string {
  if (!payload || typeof payload !== 'object') return '';
  const value = payload as any;
  if (format === 'openai-sse') {
    const content = value?.choices?.[0]?.delta?.content;
    if (typeof content === 'string') return content;
    if (Array.isArray(content)) {
      return content.map((part: any) => typeof part?.text === 'string' ? part.text : '').join('');
    }
    return '';
  }
  const content = value?.message?.content ?? value?.response;
  return typeof content === 'string' ? content : '';
}

/** Consume OpenAI SSE or Ollama newline-delimited JSON incrementally. */
export async function consumeModelStream(
  response: Response,
  format: StreamFormat,
  onChunk: (chunk: string) => void
): Promise<void> {
  if (!response.ok) {
    throw new Error(`Streaming request failed (${response.status}): ${(await response.text()).slice(0, 500)}`);
  }
  if (!response.body) throw new Error('Streaming response did not include a readable body.');

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let dataLines: string[] = [];

  const emitPayload = (raw: string): void => {
    const data = raw.trim();
    if (!data || data === '[DONE]') return;
    try {
      const chunk = extractText(JSON.parse(data), format);
      if (chunk) onChunk(chunk);
    } catch (error) {
      if (error instanceof SyntaxError) throw new Error('Provider returned malformed streaming JSON.');
      throw error;
    }
  };

  const processLine = (line: string): void => {
    const value = line.replace(/\r$/, '');
    if (format === 'openai-sse') {
      if (value.startsWith('data:')) dataLines.push(value.slice(5).trimStart());
      else if (!value && dataLines.length) {
        emitPayload(dataLines.join('\n'));
        dataLines = [];
      }
    } else if (value.trim()) {
      emitPayload(value);
    }
  };

  try {
    while (true) {
      const { value, done } = await reader.read();
      buffer += decoder.decode(value, { stream: !done });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const line of lines) processLine(line);
      if (done) break;
    }
    if (buffer) processLine(buffer);
    if (dataLines.length) emitPayload(dataLines.join('\n'));
  } finally {
    reader.releaseLock();
  }
}
