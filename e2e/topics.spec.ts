import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = {};
window.__TOPICS__ = [];
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  const topics = window.__TOPICS__;
  const clone = (t) => ({ id: t.id, name: t.name, template: t.template, lists: t.lists.map(l => ({ name: l.name, variants: [...l.variants] })) });
  const findByName = (name, excludeId) => topics.find(t => t.id !== excludeId && t.name.trim().toLowerCase() === String(name).trim().toLowerCase());
  switch (cmd) {
    case 'list_topics':
      return topics.map(clone);
    case 'create_topic': {
      const name = String(args?.payload?.name ?? '').trim();
      const template = String(args?.payload?.template ?? '').trim();
      const lists = (args?.payload?.lists ?? []).map(l => ({ name: String(l.name ?? ''), variants: (l.variants ?? []).map(String) }));
      if (!name) throw new Error('Tên chủ đề không được để trống');
      if (!template) throw new Error('Prompt mẫu không được để trống');
      if (findByName(name, null)) throw new Error("Tên chủ đề '" + name + "' đã tồn tại");
      const topic = { id: 'topic-' + (topics.length + 1) + '-' + Date.now(), name, template, lists };
      topics.push(topic);
      return clone(topic);
    }
    case 'update_topic': {
      const id = args?.payload?.id;
      const target = topics.find(t => t.id === id);
      if (!target) throw new Error("Không tìm thấy chủ đề '" + id + "'");
      const name = String(args?.payload?.name ?? '').trim();
      const template = String(args?.payload?.template ?? '').trim();
      if (!name) throw new Error('Tên chủ đề không được để trống');
      if (findByName(name, id)) throw new Error("Tên chủ đề '" + name + "' đã tồn tại");
      target.name = name;
      target.template = template;
      target.lists = (args?.payload?.lists ?? []).map(l => ({ name: String(l.name ?? ''), variants: (l.variants ?? []).map(String) }));
      return clone(target);
    }
    case 'delete_topic': {
      const id = args?.id;
      const target = topics.find(t => t.id === id);
      if (!target) throw new Error("Không tìm thấy chủ đề '" + id + "'");
      topics.splice(topics.indexOf(target), 1);
      return;
    }
    default:
      throw new Error('Unknown mock command: ' + cmd);
  }
};
`;

test.describe('Topics E2E', () => {
  test.skip(
    (!process.env.E2E_API_KEY && !process.env.CI && process.env.E2E_MOCK !== 'true'),
    'E2E_MOCK not enabled, skipping topics E2E. Set E2E_MOCK=true to run.'
  );

  test.beforeEach(async ({ page }) => {
    if (process.env.E2E_MOCK === 'true') {
      await page.addInitScript(mockImplCode);
    }
    await page.goto('/');
    const tauri = await isTauri(page);
    if (!tauri) {
      test.skip(true, 'Not in Tauri window, skip. Run via npm run tauri:dev + tauri-driver');
    }
  });

  test('list_topics is empty initially', async ({ page }) => {
    const list: any[] = await invoke(page, 'list_topics');
    expect(Array.isArray(list)).toBe(true);
    expect(list.length).toBe(0);
  });

  test('create_topic returns the topic with template and lists', async ({ page }) => {
    const created: any = await invoke(page, 'create_topic', {
      payload: {
        name: 'Nhân quả',
        template: 'viết 1 đoạn văn theo phong cách [phong cách] về chủ đề Nhân quả',
        lists: [{ name: 'phong cách', variants: ['kể chuyện', 'triết lí'] }],
      },
    });
    expect(created.name).toBe('Nhân quả');
    expect(created.template).toContain('[phong cách]');
    expect(created.id).toBeTruthy();
    expect(created.lists.length).toBe(1);
    expect(created.lists[0].name).toBe('phong cách');
    expect(created.lists[0].variants).toEqual(['kể chuyện', 'triết lí']);
  });

  test('list_topics returns the created topic', async ({ page }) => {
    await invoke(page, 'create_topic', {
      payload: { name: 'Nhân quả', template: 'Prompt A', lists: [] },
    });
    const list: any[] = await invoke(page, 'list_topics');
    expect(list.length).toBe(1);
    expect(list[0].name).toBe('Nhân quả');
    expect(list[0].template).toBe('Prompt A');
    expect(list[0].lists).toEqual([]);
  });

  test('update_topic changes name, template and list', async ({ page }) => {
    const created: any = await invoke(page, 'create_topic', {
      payload: { name: 'Nhân quả', template: 'Prompt A', lists: [{ name: 'phong cách', variants: ['kể chuyện'] }] },
    });
    const updated: any = await invoke(page, 'update_topic', {
      payload: {
        id: created.id,
        name: 'Nhân quả (sửa)',
        template: 'Prompt B [phong cách]',
        lists: [{ name: 'phong cách', variants: ['nhẹ nhàng', 'hiện đại'] }],
      },
    });
    expect(updated.id).toBe(created.id);
    expect(updated.name).toBe('Nhân quả (sửa)');
    expect(updated.template).toBe('Prompt B [phong cách]');
    expect(updated.lists[0].variants).toEqual(['nhẹ nhàng', 'hiện đại']);
    const list: any[] = await invoke(page, 'list_topics');
    expect(list[0].name).toBe('Nhân quả (sửa)');
    expect(list[0].lists[0].variants).toEqual(['nhẹ nhàng', 'hiện đại']);
  });

  test('delete_topic removes the topic', async ({ page }) => {
    const created: any = await invoke(page, 'create_topic', {
      payload: { name: 'Nhân quả', template: 'Prompt A', lists: [] },
    });
    await invoke(page, 'delete_topic', { id: created.id });
    const list: any[] = await invoke(page, 'list_topics');
    expect(list.length).toBe(0);
  });

  test('create_topic rejects invalid input', async ({ page }) => {
    try {
      await invoke(page, 'create_topic', { payload: { name: '   ', template: 'Prompt', lists: [] } });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Tên chủ đề không được để trống');
    }
    await invoke(page, 'create_topic', { payload: { name: 'Trùng', template: 'Prompt', lists: [] } });
    try {
      await invoke(page, 'create_topic', { payload: { name: 'Trùng', template: 'Prompt khác', lists: [] } });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain("đã tồn tại");
    }
  });
});
