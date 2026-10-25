// Pure CSV helpers for the report page. No Tauri imports so they stay testable
// in isolation and never touch Graph.

const FORMULA_START = /^[=+\-@\t\r]/

function cell(value) {
  const raw = value == null ? '' : String(value)
  // Neutralize spreadsheet formulas before RFC4180 quoting so a message that
  // starts with "=" or "@" cannot execute when the CSV is opened.
  const text = FORMULA_START.test(raw) ? `'${raw}` : raw
  return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text
}

function isoFromSeconds(seconds) {
  const value = Number(seconds)
  if (!Number.isFinite(value)) return ''
  const date = new Date(value * 1000)
  if (Number.isNaN(date.getTime())) return ''
  return date.toISOString()
}

function reportNotes(report) {
  const notes = []
  if (report?.truncated) notes.push('Đã đạt giới hạn 2.000 bài')
  if (report?.partial) notes.push('Một số bài thiếu summary cảm xúc/bình luận')
  return notes.join('; ')
}

export function buildReportCsv(report) {
  const safe = report || {}
  const posts = Array.isArray(safe.posts) ? safe.posts : []
  const lines = [
    ['Trang', safe.pageName || safe.pageId || ''],
    ['Page ID', safe.pageId || ''],
    ['Khoảng thời gian', `${Number(safe.days) || 0} ngày`],
    ['Từ (ISO)', isoFromSeconds(safe.since)],
    ['Đến (ISO)', isoFromSeconds(safe.until)],
    ['Tổng số bài', Number(safe.postCount) || 0],
    ['Tổng tương tác', Number(safe.totalInteractions) || 0],
    ['Cảm xúc', Number(safe.totalReactions) || 0],
    ['Bình luận', Number(safe.totalComments) || 0],
    ['Chia sẻ', Number(safe.totalShares) || 0],
    ['Ghi chú (cap/partial)', reportNotes(safe)],
    ['Xuất lúc (ISO)', new Date().toISOString()],
  ].map(([key, value]) => `${cell(key)},${cell(value)}`)

  const rows = [
    ['ID', 'Nội dung', 'Thời gian', 'Cảm xúc', 'Bình luận', 'Chia sẻ', 'Tương tác', 'Permalink'],
  ]
  for (const post of posts) {
    const reactions = Number(post?.reactions) || 0
    const comments = Number(post?.comments) || 0
    const shares = Number(post?.shares) || 0
    rows.push([
      post?.id || '',
      post?.message || '',
      post?.createdTime || '',
      reactions,
      comments,
      shares,
      reactions + comments + shares,
      post?.permalinkUrl || '',
    ])
  }

  return [...lines, '', ...rows.map(row => row.map(cell).join(','))].join('\r\n') + '\r\n'
}

function slugify(value) {
  return String(value || '')
    .replace(/[đĐ]/g, char => (char === 'đ' ? 'd' : 'D'))
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}

export function reportCsvFileName(report, now = new Date()) {
  const slug = slugify(report?.pageName) || `page-${report?.pageId || ''}`
  const days = Number(report?.days) || 0
  const pad = value => String(value).padStart(2, '0')
  const stamp = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
  return `bao-cao-${slug}-${days}ngay-${stamp}.csv`
}
