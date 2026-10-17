# Changelog

Tất cả thay đổi đáng chú ý của FlowPost AI được ghi lại ở đây. Định dạng theo [Keep a Changelog](https://keepachangelog.com/vi/1.0.0/) và dự án tuân theo [Semantic Versioning](https://semver.org/lang/vi/).

## [Unreleased] - 2026-10-17 — Fix triệt để mojibake output AI (encoding)
### Fixed
- `src-tauri/src/ai.rs` — thay `String::from_utf8_lossy` (tạo U+FFFD, phá byte) bằng `decode_response_bytes` không mất dữ liệu: UTF-8 nghiêm ngặt, fallback giải mã CP1252 (byte→char)
- `src-tauri/src/ai.rs` — viết lại `repair_mojibake` theo segment-wise: mỗi đoạn ký tự map-được CP1252 được giải mã độc lập; ký tự không map được (U+FFFD, emoji, chữ Việt đúng) đi xuyên qua và KHÔNG chặn việc phục hồi các đoạn lân cận (trước đây guard all-or-nothing khiến 1 ký tự lỗi khoá toàn bộ tiêu đề + thân bài)
- `src-tauri/src/ai.rs` — `repair_mojibake` lặp tối đa 2 lượt (`MOJIBAKE_MAX_PASSES`) để xử lý double-encoding; idempotent, không đổi văn bản Việt/ASCII đúng
- `src-tauri/src/content.rs` — `read_content_index` phục hồi mojibake khi ĐỌC (in-memory, KHÔNG ghi lại file) để bài viết cũ trong Kho nội dung hiển thị đúng
### Added
- `src-tauri/src/ai.rs` — marker `MOJIBAKE_REPAIR_VERSION = "v2"` + log khởi động trong `lib.rs run()` và log mỗi lần `generate_content` (chứng minh build mới đang chạy)
- `src-tauri/src/ai.rs` — chẩn đoán `looks_like_mojibake` + `log_mojibake_preview` (giới hạn 64 byte hex, chỉ in khi còn mojibake sau khi sửa; không in prompt/header/key)
- `src-tauri/src/ai.rs` — 9 test mới (round-trip, segment quanh U+FFFD, idempotent, double-layer, decode UTF-8/CP1252, pipeline decode→repair)
### Verified
- `cargo test --lib` 85 passed / 0 failed (76 cũ + 9 mới); `cargo check` 0 errors; `npm run build` OK. `npm run lint` bị chặn bởi quyền sandbox trong phiên QA (diff thuần Rust nên không ảnh hưởng ESLint). Kiểm thử GUI end-to-end với AI thật: người dùng cần rebuild và xác nhận.

## [Unreleased] - 2026-10-17 — Fix sửa bài viết ở Kho nội dung
### Fixed
- `src/App.jsx` — lỗi `update_content missing required key payload`: bọc tham số trong `{ payload: { contentId, title, body, prompt, style, customStyle, mediaIds } }` (khớp `save_content`)
- `src/App.jsx` — kéo dãn textarea rồi thả chuột bị đóng preview: overlay chuyển từ `onClick` sang `onMouseDown` + guard `e.target === e.currentTarget` (chỉ đóng khi bấm trực tiếp ra nền)
- `src/App.jsx` — lỗi `unknown variant Approved`: chuẩn hóa status về chữ thường `draft/approved/archived`; thêm `CONTENT_STATUS_LABELS`/`contentStatusLabel`; sửa filter ('all'), bộ đếm, badge, nút Duyệt/Lên lịch, dashboard `pending`
- `e2e/schedule.spec.ts` — status literals chuyển về chữ thường cho khớp serde backend
### Added
- `src-tauri/src/content.rs` — 4 test: serialization lowercase tuyệt đối, deserialization đúng giá trị frontend (từ chối PascalCase), persisted JSON lowercase, `UpdateContentPayload` camelCase; thay test cũ quá dễ dãi
### Verified
- `cargo test --lib` 68 passed / 0 failed; `cargo check` 0 errors (7 warning dead-code có sẵn); `npm run build` OK (2394 modules). `npm run lint` bị chặn bởi quyền sandbox (chưa xác minh độc lập). Kiểm thử GUI thủ công (AC11) chưa chạy.

## [Unreleased] - 2026-10-17 — AI content Facebook-ready formatting
### Added
- `src-tauri/src/content.rs` — `TITLE_BODY_SEPARATOR = "\n\n\n"` + `compose_facebook_post(title, body)` (ghép tiêu đề + 2 dòng trống + thân bài; xử lý rỗng) + 2 tests
- `src-tauri/src/ai.rs` — `strip_markdown` (loại bỏ `#`/`**`/`*`/`_`/`` ` ``/`>`/bullet, không dùng regex); `build_system_message` thêm 5 quy tắc định dạng (plain text, KHÔNG markdown, tiêu đề IN HOA, cách 2 dòng); `parse_ai_response` chuẩn hóa title=UPPERCASE + strip markdown; thêm tests
### Changed
- `src-tauri/src/schedule.rs` — publish dùng `compose_facebook_post(&found.title, &found.body)` tại `read_content_with_media` + `read_content_body` (trước đây tiêu đề bị bỏ, chỉ đăng body)
- `src/App.jsx` — preview body thêm `whiteSpace:'pre-line'` để giữ xuống dòng/2 dòng trống
### Fixed
- `###`/`**` hiển thị nguyên văn trên Facebook → nay AI bắt buộc plain text, có lớp phòng thủ strip markdown ở Rust
- Tiêu đề không còn dính liền thân bài (trước đây newline bị gộp và tiêu đề không được đăng)
- Gộp các fix pipeline AI có sẵn trong working tree: parse SSE streaming (`data:`/`[DONE]`), endpoint `/chat/completions` (tránh double suffix), `localhost`→`127.0.0.1` (tránh IPv6), `AI_TIMEOUT_SECS` 120s, log lỗi; `providers.rs` base URL `127.0.0.1` + `.http1_only()`
### Verified
- `cargo test --lib` 65 passed / 0 failed; `cargo check` 0 errors; `npm run build` OK (2394 modules). `npm run lint` bị chặn bởi quyền công cụ (chưa xác minh độc lập trong phiên QA).

## [1be13e2] - 2026-09-05 — B1 CI + lint fixes
### Added
- `.github/workflows/ci.yml` — single-job CI (ubuntu-latest, Node 20, npm ci, lint, build, Rust stable + cargo check/test, Playwright E2E_MOCK 19 tests, Vite dev server, upload artifact on failure)
### Fixed
- `src/App.jsx` — 7 lỗi eslint (di chuyển `selectedPageId` trước `refresh`, `catch (_e) { void _e }`, disable `set-state-in-effect`), `npm run lint` pass, `npm run build` success

## [d8987ba] - 2026-09-05 — A4 reschedule/cancel + A5 E2E publish/schedule mock
### Added
- `src/App.jsx` SchedulePage — `platformFilter` (Tất cả/3 platforms), badge `Cancelled` xám, Queue 3 nút Dời/Hủy/Xóa, Calendar dayDetail + rescheduleFor modal, `handleReschedule` → `reschedule`, `cancelItem` → `update_schedule_status Cancelled`
- `e2e/publish.spec.ts` (4 tests) — mock `validate_facebook_token`, `list_facebook_pages`, `publish_content` (check `.exe`)
- `e2e/schedule.spec.ts` (5 tests) — in-memory `mockContents`/`mockSchedules`, mock `create_schedule` (Approved + 5m + pageId), `reschedule`, `Cancelled`
### Changed
- E2E suite từ 8 lên 19 tests (ai 3, providers 6, publish 4, schedule 5) qua `E2E_MOCK=true`

## [a569771] - 2026-09-05 — A3 scheduler hardening
### Added
- `src-tauri/src/schedule.rs` — `resolve_first_image_path` (đọc `media-index.json`, check image/mime/10MB, fallback `None`), `read_content_with_media`, helper `first_image` cho `process_due_schedules`
- `src-tauri/src/schedule.rs` — `AtomicBool SCHEDULER_RUNNING` dedupe `start_scheduler`, hot-reload `interval_seconds` mỗi tick, `set_scheduler_config` validate `1..=3600`
### Fixed
- `process_due_schedules` trước luôn `publish_content(..., None)` bỏ qua ảnh — nay gửi ảnh đầu nếu có

## [2ca7c30] - 2026-09-05 — A2 wiring pageId
### Added
- `src-tauri/src/schedule.rs` — `CreateSchedulePayload` thêm `page_id: Option<String>`, `create_schedule` resolve `pageId = pageId.trim().or(pages.first())`, giữ backward compat JSON cũ
- `src/App.jsx` `ScheduleModal` — dropdown `PageInfo[]` từ `list_facebook_pages`, default `get_scheduler_config.selectedPageId`, gửi `pageId` + `pages:[pageId]`, validate `pageId` trống
- `src/App.jsx` `SettingsPage` — selector `Trang mặc định cho Lịch` gọi `select_scheduler_page`, persist `schedule-config.json`

## [3b6a43d] - 2026-09-05 — A1 PublishModal picker 1 ảnh đầu
### Added
- `src/App.jsx` `PublishModal` — picker 1 ảnh (`list_media` + `localAssetUrl`, `selectedMediaId`, `showPicker`), `handlePublish` lấy `found.path` truyền `image_path` (giữ `null` nếu không chọn), kích hoạt nhánh `facebook.rs:191` `photos` trước đó chết

## [a7b622f] - 2026-09-05 — Multi-provider + API thật
### Added
- `src-tauri/src/providers.rs` — bỏ MVP limit, `DEFAULT_PROTOCOL`, 3 protocols `openai-completions/responses`, `anthropic-messages`, cho phép multi-provider `create`, 3 tests mới
- `src-tauri/src/ai.rs` — routing 3 protocols (`endpoint_for_protocol`: `/chat/completions`, `/responses` với `instructions`, `/messages` với `system` + `x-api-key`), 5 tests mới (`endpoint_for_protocol`, `responses/anthropic serialization`, `parse responses/anthropic`)
- `src/App.jsx` — enable 3 protocols dropdown, xóa Mock demo notice, subtitle cập nhật, `setMsg('Đã tạo nội dung')`
- `e2e/providers.spec.ts` — mock hỗ trợ 3 protocols, thêm 2 tests multi-provider/anthropic
- `.gitignore` — thêm `test-results/`, `playwright-report/`
### Fixed
- `cargo check` 0 warning, `cargo test` 51 passed (tăng từ 46)

## [31874ad] - 2026-09-05 — E2E_MOCK mode
### Added
- `e2e/ai.spec.ts`, `e2e/providers.spec.ts` — `window.__TAURI_MOCK_IMPL__` qua `addInitScript`, cho phép `E2E_MOCK=true npx playwright test` không cần `tauri-driver`, `helpers/tauri.ts` + `provider.ts`

## [370d1bb] - 2026-09-05 — Security key hygiene
### Fixed
- Xóa API key thật khỏi git working tree (vẫn tồn tại trong history `8bd6850`, cần filter-repo riêng)

## [0.3.0] - 2026-09-06
### Changed
- Promote `0.3.0-alpha` → `0.3.0` stable: đồng bộ `package.json`/`Cargo.toml`/`tauri.conf.json` sang `0.3.0`, không đổi code, CI đã xanh `73760b8`+`a561ff4`

## [0.3.0-alpha] - 2026-09-06
### Added
- `LICENSE` MIT 2026 FlowPost AI, `SECURITY.md` bảng hỗ trợ version + lưu secret qua keyring
- `src/App.jsx` Dashboard — C1.1 local aggregation (bỏ mock `5.660`/`12.4K`), C1.2+1.3 `list_page_posts`/`get_post_insights` 6 metric, C2 tooltip 2 cột + `StatCard` tổng tương tác từ insights
- `src/App.jsx` Dashboard — `Tỷ lệ tương tác` từ insights (`totalEngaged/totalUnique*100`, `5a267a9`), fallback `—` khi chưa có token
- `src/App.jsx` fix(providers) — cô lập lỗi tab `code 190`, list card `Danh sách Providers`, `activeId`/`+ Thêm Provider`
- `README.md` — hướng dẫn lấy Facebook Token 5 scopes (`a561ff4`) sau blockquote Tauri
### Changed
- Version `0.3.0-alpha` đồng bộ `package.json`/`Cargo.toml`/`tauri.conf.json`, badge CI `branch=dev` (sẽ chuyển `main` ở R3)
- CI đã xanh `73760b8` (cargo test 51, E2E_MOCK 19) sau fix `libdbus` + `facebook mock` + `E2E dummy key`
