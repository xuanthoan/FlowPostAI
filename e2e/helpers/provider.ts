import { invoke } from './tauri';
import type { Page } from '@playwright/test';

export const DEFAULT_ID = 'custom-provider';
export const DEFAULT_BASE_URL = process.env.E2E_BASE_URL || 'http://127.0.0.1:20128/v1';
export const DEFAULT_MODEL = process.env.E2E_MODEL || 'xoay-vong-worker-web-128k';
export const DEFAULT_API_KEY = process.env.E2E_API_KEY || '';
export const DEFAULT_PROTOCOL = process.env.E2E_PROTOCOL || 'openai-completions';

export async function ensureProvider(
  page: Page,
  opts: { id?: string; baseUrl?: string; apiKey?: string; models?: string[] } = {}
) {
  const id = opts.id || DEFAULT_ID;
  const baseUrl = opts.baseUrl || DEFAULT_BASE_URL;
  let apiKey = opts.apiKey || DEFAULT_API_KEY;
  if (!apiKey) {
    if (process.env.E2E_MOCK === 'true') apiKey = 'mock_dummy_key_for_ci';
    else throw new Error('E2E_API_KEY not set. Set env E2E_API_KEY=sk-... before running E2E.');
  }
  const models = opts.models || [DEFAULT_MODEL];

  // Try get active, then update. If no provider file, update will auto-create default.
  try {
    await invoke(page, 'update_provider', {
      id,
      payload: {
        displayName: 'Custom provider',
        baseUrl,
        protocol: DEFAULT_PROTOCOL,
        models,
        apiKey,
      },
    });
  } catch (e) {
    // If provider not found (fresh install), try create
    const msg = String(e);
    if (msg.includes('Không tìm thấy provider')) {
      await invoke(page, 'create_provider', {
        payload: {
          id,
          displayName: 'Custom provider',
          baseUrl,
          protocol: DEFAULT_PROTOCOL,
          models,
          apiKey,
        },
      });
    } else {
      throw e;
    }
  }

  const active = await invoke<any>(page, 'get_active_provider');
  if (!active.hasApiKey) throw new Error('ensureProvider: hasApiKey false after update');
  return active;
}

export async function listProviders(page: Page) {
  return invoke<any[]>(page, 'list_providers');
}
