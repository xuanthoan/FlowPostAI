export const DEFAULT_LOCAL_BASE_URL = 'http://127.0.0.1:20128/v1'

// Preset catalog for the AI provider onboarding form.
// `models` are ADVISORY suggestions only — the provider's own /models endpoint is the source of truth.
export const PROVIDER_PRESETS = [
  {
    key: 'chatgpt',
    label: 'ChatGPT (OpenAI)',
    id: 'openai',
    displayName: 'ChatGPT (OpenAI)',
    baseUrl: 'https://api.openai.com/v1',
    protocol: 'openai-completions',
    models: ['gpt-4o', 'gpt-4o-mini'],
    docsUrl: 'https://platform.openai.com/api-keys',
  },
  {
    key: 'gemini',
    label: 'Google Gemini',
    id: 'gemini',
    displayName: 'Google Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
    protocol: 'openai-completions',
    models: ['gemini-2.5-flash', 'gemini-3.5-flash'],
    docsUrl: 'https://aistudio.google.com/app/apikey',
  },
  {
    key: 'claude',
    label: 'Anthropic Claude',
    id: 'claude',
    displayName: 'Anthropic Claude',
    baseUrl: 'https://api.anthropic.com/v1',
    protocol: 'openai-completions',
    models: ['claude-sonnet-4-6'],
    docsUrl: 'https://console.anthropic.com/settings/keys',
  },
  {
    key: 'deepseek',
    label: 'DeepSeek',
    id: 'deepseek',
    displayName: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com',
    protocol: 'openai-completions',
    models: ['deepseek-flash', 'deepseek-v4-pro'],
    docsUrl: 'https://platform.deepseek.com/api_keys',
  },
  {
    key: 'muse',
    label: 'Muse (Meta Model API)',
    id: 'muse',
    displayName: 'Muse (Meta Model API)',
    baseUrl: 'https://api.meta.ai/v1',
    protocol: 'openai-completions',
    models: ['muse-spark-1.3', 'muse-spark-1.1'],
    docsUrl: 'https://dev.meta.ai/',
  },
  {
    key: 'custom',
    label: 'Custom / OpenAI-compatible',
    id: '',
    displayName: '',
    baseUrl: DEFAULT_LOCAL_BASE_URL,
    protocol: 'openai-completions',
    models: [],
    docsUrl: '',
  },
]

// Mirrors the backend rule: only the local default gateway may be saved with zero models,
// because the backend injects its default model there.
export function isDefaultLocalBase(baseUrl) {
  const norm = String(baseUrl || '')
    .trim()
    .replace('localhost', '127.0.0.1')
    .replace(/\/+$/, '')
  return norm === DEFAULT_LOCAL_BASE_URL
}
