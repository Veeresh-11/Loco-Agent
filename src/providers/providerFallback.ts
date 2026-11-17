export interface ProviderFailure<T> {
  provider: T;
  error: string;
}

export interface ProviderFallbackResult<T> {
  value?: string;
  provider?: T;
  failures: ProviderFailure<T>[];
}

export async function runWithProviderFallback<T extends string>(
  providers: readonly T[],
  attempt: (provider: T) => Promise<string | null | undefined>
): Promise<ProviderFallbackResult<T>> {
  const failures: ProviderFailure<T>[] = [];

  for (const provider of providers) {
    try {
      const value = (await attempt(provider))?.trim();
      if (value) return { value, provider, failures };
      failures.push({ provider, error: 'Provider returned an empty response.' });
    } catch (error) {
      failures.push({
        provider,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  return { failures };
}