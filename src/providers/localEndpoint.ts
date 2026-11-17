export interface LocalApiRoute {
  path: string;
  format: 'openai' | 'ollama-chat' | 'ollama-generate';
}

export function normalizeLocalEndpoint(endpoint: string): string {
  return endpoint.trim().replace(/\/+$/, '');
}

export function getLocalProbePaths(endpoint: string): string[] {
  const base = normalizeLocalEndpoint(endpoint);
  return base.endsWith('/v1')
    ? ['/models', '/api/tags', '/health']
    : ['/api/tags', '/v1/models', '/health'];
}

export function getLocalChatRoutes(endpoint: string): LocalApiRoute[] {
  const base = normalizeLocalEndpoint(endpoint);
  const openAiPath = base.endsWith('/v1') ? '/chat/completions' : '/v1/chat/completions';
  return [
    { path: openAiPath, format: 'openai' },
    { path: '/api/chat', format: 'ollama-chat' },
    { path: '/api/generate', format: 'ollama-generate' }
  ];
}