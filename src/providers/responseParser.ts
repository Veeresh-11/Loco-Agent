export function parseOpenAICompatibleResponse(payload: unknown): string {
  if (!payload || typeof payload !== 'object') return '';
  const value = payload as any;
  const content = value?.choices?.[0]?.message?.content ?? value?.output_text;
  if (typeof content === 'string') return content.trim();
  if (Array.isArray(content)) {
    return content
      .map((part: unknown) => {
        if (typeof part === 'string') return part;
        if (part && typeof part === 'object' && typeof (part as any).text === 'string') return (part as any).text;
        return '';
      })
      .filter(Boolean)
      .join('')
      .trim();
  }
  return '';
}
